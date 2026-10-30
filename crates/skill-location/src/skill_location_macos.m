// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Objective-C bridge for CoreLocation on macOS.
// Exposes a blocking C API consumed by the Rust FFI layer.
//
// ── Concurrency model ───────────────────────────────────────────────────────
//
// CLLocationManager delivers delegate callbacks on the run loop of the thread
// that created it, so every call below creates its manager on the CALLING
// thread and spins that thread's run loop until done or timed out.
//
// Never hop to the main queue: skill-daemon's main thread runs tokio and never
// drains the main dispatch queue, so dispatch_sync(main) hung forever (POST
// /v1/settings/location-enabled never returned) and dispatch_async(main) +
// semaphore always burned the full timeout + 5 s before the IP fallback.

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

// Spin the current thread's run loop until `del.done` or `timeout_secs` elapse.
static void spinUntilDone(SkillLocationDelegate *del, double timeout_secs) {
    NSDate *deadline = [NSDate dateWithTimeIntervalSinceNow:timeout_secs];
    while (!del.done && [[NSDate date] compare:deadline] == NSOrderedAscending) {
        // runMode returns NO at once when the loop has no sources; sleep then
        // so a thread without CoreLocation sources does not busy-spin.
        if (![[NSRunLoop currentRunLoop] runMode:NSDefaultRunLoopMode
                                      beforeDate:[NSDate dateWithTimeIntervalSinceNow:0.05]]) {
            [NSThread sleepForTimeInterval:0.05];
        }
    }
}

int skill_location_auth_status(void) {
    CLAuthorizationStatus status;
    @autoreleasepool {
        status = [[CLLocationManager alloc] init].authorizationStatus;
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
    @autoreleasepool {
        CLLocationManager *mgr = [[CLLocationManager alloc] init];
        if (isAuthorized(mgr.authorizationStatus)) return 1;
        if (mgr.authorizationStatus != kCLAuthorizationStatusNotDetermined) return 0;

        SkillLocationDelegate *del = [[SkillLocationDelegate alloc] init];
        del.authStatus = mgr.authorizationStatus;
        mgr.delegate = del;
        [mgr requestWhenInUseAuthorization];
        spinUntilDone(del, timeout_secs);
        return isAuthorized(del.authStatus) ? 1 : 0;
    }
}

/// Fetch the current location.  Blocks the calling thread for up to `timeout_secs`.
void skill_location_fetch(double timeout_secs, SkillLocationResult *out) {
    memset(out, 0, sizeof(*out));
    out->altitude            = NAN;
    out->horizontal_accuracy = -1;
    out->vertical_accuracy   = -1;
    out->speed               = -1;
    out->course              = -1;

    @autoreleasepool {
        CLLocationManager *mgr = [[CLLocationManager alloc] init];
        out->auth_status = (int)mgr.authorizationStatus;
        if (!isAuthorized(mgr.authorizationStatus)) {
            snprintf(out->error, sizeof(out->error),
                     "location not authorized (status=%d)", (int)mgr.authorizationStatus);
            return;
        }

        SkillLocationDelegate *del = [[SkillLocationDelegate alloc] init];
        del.authStatus = mgr.authorizationStatus;
        mgr.delegate = del;
        mgr.desiredAccuracy = kCLLocationAccuracyBest;
        [mgr requestLocation];
        spinUntilDone(del, timeout_secs);
        mgr.delegate = nil;

        if (del.lastLocation) {
            CLLocation *loc = del.lastLocation;
            out->ok                  = 1;
            out->latitude            = loc.coordinate.latitude;
            out->longitude           = loc.coordinate.longitude;
            out->altitude            = loc.altitude;
            out->horizontal_accuracy = loc.horizontalAccuracy;
            out->vertical_accuracy   = loc.verticalAccuracy;
            out->speed               = loc.speed;
            out->course              = loc.course;
            out->timestamp           = [loc.timestamp timeIntervalSince1970];
        } else if (del.lastError) {
            snprintf(out->error, sizeof(out->error),
                     "%s", del.lastError.localizedDescription.UTF8String ?: "unknown error");
        } else {
            snprintf(out->error, sizeof(out->error), "location request timed out");
        }
    }
}
