// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Objective-C bridge for CoreLocation on macOS.
// Exposes a blocking C API consumed by the Rust FFI layer.
//
// ── Concurrency model ───────────────────────────────────────────────────────
//
// CLLocationManager must be created on a thread that has an active run loop.
// Delegate callbacks are delivered on that same thread/run loop.
//
// The Rust callers come from tokio::task::spawn_blocking threads, which do NOT
// have a run loop.  The naive fix — dispatch_sync(main_queue, ^{ spin_runloop })
// — deadlocks: CoreLocation dispatches delegate callbacks via
// dispatch_async(main_queue,...) but the main queue is already occupied by the
// dispatch_sync block, so callbacks never fire and the spin loop times out.
//
// Correct fix: dispatch_async(main_queue, work) + dispatch_semaphore_wait on
// the calling thread.  The main queue is free to receive CoreLocation callbacks
// while the work block is spinning the run loop, because the spin itself calls
// [[NSRunLoop currentRunLoop] runMode:...] which also drains pending dispatch
// queue sources (including those callbacks).

#import <CoreLocation/CoreLocation.h>
#import <Foundation/Foundation.h>

// ── Result struct returned across FFI ────────────────────────────────────────

typedef struct {
    int    ok;                  // 1 = success, 0 = error
    double latitude;            // WGS-84 degrees
    double longitude;
    double altitude;            // metres (NaN if unavailable)
    double horizontal_accuracy; // metres  (-1 if unavailable)
    double vertical_accuracy;   // metres  (-1 if unavailable)
    double speed;               // m/s     (-1 if unavailable)
    double course;              // degrees (-1 if unavailable)
    double timestamp;           // unix seconds (double)
    int    auth_status;         // CLAuthorizationStatus raw value
    char   error[256];          // NUL-terminated error message (empty on success)
} SkillLocationResult;

// ── Auth status enum (matches Rust LocationAuthStatus) ───────────────────────

typedef enum {
    SkillLocAuthNotDetermined = 0,
    SkillLocAuthRestricted    = 1,
    SkillLocAuthDenied        = 2,
    SkillLocAuthAuthorized    = 3,
} SkillLocAuthStatus;

// ── Helper: is this status considered "authorized"? ──────────────────────────

static BOOL isAuthorized(CLAuthorizationStatus s) {
    // On macOS the only granted status is AuthorizedAlways.
    // kCLAuthorizationStatusAuthorizedWhenInUse is iOS-only.
    return s == kCLAuthorizationStatusAuthorizedAlways;
}

// ── CLLocationManager delegate ───────────────────────────────────────────────

@interface SkillLocationDelegate : NSObject <CLLocationManagerDelegate>
@property (nonatomic, strong) CLLocation *lastLocation;
@property (nonatomic, strong) NSError    *lastError;
@property (nonatomic, assign) BOOL        done;
@property (nonatomic, assign) CLAuthorizationStatus authStatus;
@end

@implementation SkillLocationDelegate

- (void)locationManager:(CLLocationManager *)manager
     didUpdateLocations:(NSArray<CLLocation *> *)locations {
    if (locations.count > 0) {
        self.lastLocation = locations.lastObject;
    }
    self.done = YES;
}

- (void)locationManager:(CLLocationManager *)manager
       didFailWithError:(NSError *)error {
    self.lastError = error;
    self.done = YES;
}

- (void)locationManagerDidChangeAuthorization:(CLLocationManager *)manager {
    self.authStatus = manager.authorizationStatus;
    // Wake the spin loop for any terminal state (granted or denied).
    if (self.authStatus != kCLAuthorizationStatusNotDetermined) {
        self.done = YES;
    }
}

@end

// ── Public C API ─────────────────────────────────────────────────────────────

// auth_status is a quick property read — dispatch_sync is fine here because
// it holds no run loop and returns immediately.
int skill_location_auth_status(void) {
    __block CLAuthorizationStatus status;

    void (^read)(void) = ^{
        CLLocationManager *mgr = [[CLLocationManager alloc] init];
        status = mgr.authorizationStatus;
    };

    if ([NSThread isMainThread]) {
        read();
    } else {
        dispatch_sync(dispatch_get_main_queue(), read);
    }

    switch (status) {
        case kCLAuthorizationStatusNotDetermined:     return SkillLocAuthNotDetermined;
        case kCLAuthorizationStatusRestricted:        return SkillLocAuthRestricted;
        case kCLAuthorizationStatusDenied:            return SkillLocAuthDenied;
        case kCLAuthorizationStatusAuthorizedAlways:  // fall-through
        default:                                      return SkillLocAuthAuthorized;
    }
}

/// Request location permission.
/// Returns 1 if access is (or becomes) authorized, 0 otherwise.
/// Blocks the calling thread for up to `timeout_secs` seconds.
int skill_location_request_access(double timeout_secs) {
    if ([NSThread isMainThread]) {
        // Direct call — already on the main thread.
        CLLocationManager *mgr = [[CLLocationManager alloc] init];
        if (isAuthorized(mgr.authorizationStatus)) return 1;
        if (mgr.authorizationStatus != kCLAuthorizationStatusNotDetermined) return 0;

        SkillLocationDelegate *del = [[SkillLocationDelegate alloc] init];
        del.authStatus = mgr.authorizationStatus;
        mgr.delegate = del;
        [mgr requestWhenInUseAuthorization];

        NSDate *deadline = [NSDate dateWithTimeIntervalSinceNow:timeout_secs];
        while (!del.done &&
               [[NSDate date] compare:deadline] == NSOrderedAscending) {
            [[NSRunLoop currentRunLoop] runMode:NSDefaultRunLoopMode
                                     beforeDate:[NSDate dateWithTimeIntervalSinceNow:0.05]];
        }
        return isAuthorized(del.authStatus) ? 1 : 0;
    }

    // Called from a background thread: use dispatch_async so the main queue
    // remains free to receive CoreLocation delegate callbacks.
    dispatch_semaphore_t sem = dispatch_semaphore_create(0);
    __block int result = 0;

    dispatch_async(dispatch_get_main_queue(), ^{
        CLLocationManager *mgr = [[CLLocationManager alloc] init];
        if (isAuthorized(mgr.authorizationStatus)) {
            result = 1;
            dispatch_semaphore_signal(sem);
            return;
        }
        if (mgr.authorizationStatus != kCLAuthorizationStatusNotDetermined) {
            result = 0;
            dispatch_semaphore_signal(sem);
            return;
        }

        SkillLocationDelegate *del = [[SkillLocationDelegate alloc] init];
        del.authStatus = mgr.authorizationStatus;
        mgr.delegate = del;
        [mgr requestWhenInUseAuthorization];

        NSDate *deadline = [NSDate dateWithTimeIntervalSinceNow:timeout_secs];
        while (!del.done &&
               [[NSDate date] compare:deadline] == NSOrderedAscending) {
            // runMode:beforeDate: also drains pending dispatch_async sources,
            // so CoreLocation delegate callbacks arrive here without deadlock.
            [[NSRunLoop currentRunLoop] runMode:NSDefaultRunLoopMode
                                     beforeDate:[NSDate dateWithTimeIntervalSinceNow:0.05]];
        }

        result = isAuthorized(del.authStatus) ? 1 : 0;
        dispatch_semaphore_signal(sem);
    });

    // Wait slightly longer than the run-loop timeout to account for scheduling.
    dispatch_time_t wait = dispatch_time(DISPATCH_TIME_NOW,
                                         (int64_t)((timeout_secs + 5.0) * NSEC_PER_SEC));
    dispatch_semaphore_wait(sem, wait);
    return result;
}

/// Fetch the current location.  Blocks the calling thread for up to `timeout_secs`.
void skill_location_fetch(double timeout_secs, SkillLocationResult *out) {
    memset(out, 0, sizeof(*out));
    out->altitude            = NAN;
    out->horizontal_accuracy = -1;
    out->vertical_accuracy   = -1;
    out->speed               = -1;
    out->course              = -1;

    void (^work)(SkillLocationResult *) = ^(SkillLocationResult *r) {
        CLLocationManager *mgr = [[CLLocationManager alloc] init];
        SkillLocationDelegate *del = [[SkillLocationDelegate alloc] init];
        del.authStatus = mgr.authorizationStatus;
        mgr.delegate = del;
        mgr.desiredAccuracy = kCLLocationAccuracyBest;

        r->auth_status = (int)mgr.authorizationStatus;

        if (!isAuthorized(mgr.authorizationStatus)) {
            r->ok = 0;
            snprintf(r->error, sizeof(r->error),
                     "location not authorized (status=%d)", (int)mgr.authorizationStatus);
            return;
        }

        [mgr requestLocation];

        NSDate *deadline = [NSDate dateWithTimeIntervalSinceNow:timeout_secs];
        while (!del.done &&
               [[NSDate date] compare:deadline] == NSOrderedAscending) {
            [[NSRunLoop currentRunLoop] runMode:NSDefaultRunLoopMode
                                     beforeDate:[NSDate dateWithTimeIntervalSinceNow:0.05]];
        }

        if (del.lastLocation) {
            CLLocation *loc = del.lastLocation;
            r->ok                  = 1;
            r->latitude            = loc.coordinate.latitude;
            r->longitude           = loc.coordinate.longitude;
            r->altitude            = loc.altitude;
            r->horizontal_accuracy = loc.horizontalAccuracy;
            r->vertical_accuracy   = loc.verticalAccuracy;
            r->speed               = loc.speed;
            r->course              = loc.course;
            r->timestamp           = [loc.timestamp timeIntervalSince1970];
        } else if (del.lastError) {
            r->ok = 0;
            snprintf(r->error, sizeof(r->error),
                     "%s", del.lastError.localizedDescription.UTF8String ?: "unknown error");
        } else {
            r->ok = 0;
            snprintf(r->error, sizeof(r->error), "location request timed out");
        }
    };

    if ([NSThread isMainThread]) {
        work(out);
        return;
    }

    // Background thread path: dispatch_async + semaphore (see module comment).
    dispatch_semaphore_t sem = dispatch_semaphore_create(0);
    __block SkillLocationResult local = *out;

    dispatch_async(dispatch_get_main_queue(), ^{
        work(&local);
        dispatch_semaphore_signal(sem);
    });

    dispatch_time_t wait = dispatch_time(DISPATCH_TIME_NOW,
                                         (int64_t)((timeout_secs + 5.0) * NSEC_PER_SEC));
    dispatch_semaphore_wait(sem, wait);
    *out = local;
}
