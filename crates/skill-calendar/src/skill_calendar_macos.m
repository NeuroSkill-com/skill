// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Apple EventKit calendar bridge — compiled Objective-C with ARC.
// Exposes C functions callable from Rust via FFI.
//
// Supports macOS 10.15+ (deployment target already set to 10.15 workspace-wide).
// On macOS 14+, uses the new `requestFullAccessToEventsWithCompletion:` API.

#import <Foundation/Foundation.h>
#import <EventKit/EventKit.h>
#include <stdlib.h>
#include <string.h>

// ── Permission helpers ────────────────────────────────────────────────────────

/// Return current authorization status as an integer:
///   0 = not_determined
///   1 = authorized (or fullAccess on macOS 14+)
///   2 = denied
///   3 = restricted
///   4 = write_only (macOS 14+)
int32_t skill_calendar_auth_status(void) {
    EKAuthorizationStatus s =
        [EKEventStore authorizationStatusForEntityType:EKEntityTypeEvent];

    // EKAuthorizationStatusFullAccess = 3 on macOS 14+.
    // EKAuthorizationStatusAuthorized = 3 on older SDKs.
    // Both map to "authorized" in our API.
    switch ((NSInteger)s) {
        case 0:  return 0; // notDetermined
        case 3:  return 1; // authorized / fullAccess
        case 2:  return 2; // denied
        case 1:  return 3; // restricted
        case 4:  return 4; // writeOnly (macOS 14+)
        default: return 0;
    }
}

/// Request full calendar access, blocking until the dialog is resolved.
/// Returns 1 if granted, 0 otherwise.  Timeout: 30 seconds.
int32_t skill_calendar_request_access(void) {
    @autoreleasepool {
        EKEventStore *store = [[EKEventStore alloc] init];

        __block BOOL granted = NO;
        dispatch_semaphore_t sema = dispatch_semaphore_create(0);

        if (@available(macOS 14.0, *)) {
            [store requestFullAccessToEventsWithCompletion:^(BOOL g, NSError *__unused err) {
                granted = g;
                dispatch_semaphore_signal(sema);
            }];
        } else {
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
            [store requestAccessToEntityType:EKEntityTypeEvent
                                  completion:^(BOOL g, NSError *__unused err) {
                granted = g;
                dispatch_semaphore_signal(sema);
            }];
#pragma clang diagnostic pop
        }

        dispatch_semaphore_wait(sema, dispatch_time(DISPATCH_TIME_NOW,
                                                    30LL * NSEC_PER_SEC));
        return granted ? 1 : 0;
    }
}

// ── Event fetching ────────────────────────────────────────────────────────────

/// Fetch calendar events in [start_utc, end_utc] and return them as a
/// malloc'd UTF-8 JSON byte array (array of event objects).
///
/// On access denial the returned JSON is `{"error":"calendar_access_denied"}`.
/// Returns NULL only on internal allocation failure.
/// Caller must `free()` the returned pointer.
char *skill_calendar_fetch_events(int64_t start_utc, int64_t end_utc,
                                  uint32_t *out_len) {
    @autoreleasepool {
        EKEventStore *store = [[EKEventStore alloc] init];

        // ── Request access if needed ─────────────────────────────────────────
        EKAuthorizationStatus status =
            [EKEventStore authorizationStatusForEntityType:EKEntityTypeEvent];

        // notDetermined or writeOnly → request
        if ((NSInteger)status == 0 || (NSInteger)status == 4) {
            dispatch_semaphore_t sema = dispatch_semaphore_create(0);

            if (@available(macOS 14.0, *)) {
                [store requestFullAccessToEventsWithCompletion:^(BOOL __unused g,
                                                                 NSError *__unused err) {
                    dispatch_semaphore_signal(sema);
                }];
            } else {
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
                [store requestAccessToEntityType:EKEntityTypeEvent
                                      completion:^(BOOL __unused g, NSError *__unused err) {
                    dispatch_semaphore_signal(sema);
                }];
#pragma clang diagnostic pop
            }

            dispatch_semaphore_wait(sema,
                dispatch_time(DISPATCH_TIME_NOW, 30LL * NSEC_PER_SEC));
            status = [EKEventStore authorizationStatusForEntityType:EKEntityTypeEvent];
        }

        // denied / restricted → return error object
        if ((NSInteger)status != 3 && (NSInteger)status != 4) {
            // 3 = authorized/fullAccess   4 = writeOnly (still readable? no)
            // For status == 2 (denied) or 1 (restricted) → error
            if ((NSInteger)status == 2 || (NSInteger)status == 1) {
                const char *err = "{\"error\":\"calendar_access_denied\"}";
                size_t n = strlen(err);
                char *out = (char *)malloc(n + 1);
                if (!out) return NULL;
                memcpy(out, err, n + 1);
                if (out_len) *out_len = (uint32_t)n;
                return out;
            }
        }

        // ── Build predicate ──────────────────────────────────────────────────
        NSDate *startDate =
            [NSDate dateWithTimeIntervalSince1970:(NSTimeInterval)start_utc];
        NSDate *endDate =
            [NSDate dateWithTimeIntervalSince1970:(NSTimeInterval)end_utc];

        NSPredicate *pred =
            [store predicateForEventsWithStartDate:startDate
                                           endDate:endDate
                                         calendars:nil];
        NSArray<EKEvent *> *events = [store eventsMatchingPredicate:pred];

        // ── Serialise events ─────────────────────────────────────────────────
        NSMutableArray<NSDictionary *> *arr = [NSMutableArray array];

        for (EKEvent *ev in events) {
            NSMutableDictionary *d = [NSMutableDictionary dictionary];

            d[@"id"]        = ev.eventIdentifier ?: @"";
            d[@"title"]     = ev.title ?: @"";
            d[@"start_utc"] = @((int64_t)[ev.startDate timeIntervalSince1970]);
            d[@"end_utc"]   = @((int64_t)[ev.endDate timeIntervalSince1970]);
            d[@"all_day"]   = @(ev.allDay);

            if (ev.location.length > 0) d[@"location"] = ev.location;
            if (ev.notes.length > 0)    d[@"notes"]    = ev.notes;
            if (ev.calendar.title.length > 0) d[@"calendar"] = ev.calendar.title;

            NSString *statusStr = @"confirmed";
            switch (ev.status) {
                case EKEventStatusTentative: statusStr = @"tentative";  break;
                case EKEventStatusCanceled:  statusStr = @"cancelled";  break;
                default: break;
            }
            d[@"status"] = statusStr;

            // Recurrence rule (raw string, not expanded)
            if (ev.recurrenceRules.count > 0) {
                EKRecurrenceRule *rule = ev.recurrenceRules.firstObject;
                // Rebuild a simple RRULE string from the parsed rule
                NSString *freq = @"DAILY";
                switch (rule.frequency) {
                    case EKRecurrenceFrequencyWeekly:  freq = @"WEEKLY";  break;
                    case EKRecurrenceFrequencyMonthly: freq = @"MONTHLY"; break;
                    case EKRecurrenceFrequencyYearly:  freq = @"YEARLY";  break;
                    default: break;
                }
                NSMutableString *rrule = [NSMutableString stringWithFormat:@"FREQ=%@", freq];
                if (rule.interval > 1) {
                    [rrule appendFormat:@";INTERVAL=%ld", (long)rule.interval];
                }
                d[@"recurrence"] = rrule;
            }

            [arr addObject:d];
        }

        // ── JSON serialisation ───────────────────────────────────────────────
        NSError *jsonErr = nil;
        NSData *jsonData =
            [NSJSONSerialization dataWithJSONObject:arr options:0 error:&jsonErr];
        if (!jsonData) return NULL;

        NSUInteger n = [jsonData length];
        char *out = (char *)malloc(n + 1);
        if (!out) return NULL;
        memcpy(out, [jsonData bytes], n);
        out[n] = '\0';
        if (out_len) *out_len = (uint32_t)n;
        return out;
    }
}
