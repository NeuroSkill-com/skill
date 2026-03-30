// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Objective-C bridge for CoreLocation on macOS.
// Exposes a blocking C API consumed by the Rust FFI layer.

#import <CoreLocation/CoreLocation.h>
#import <Foundation/Foundation.h>

// ── Result struct returned across FFI ────────────────────────────────────────

typedef struct {
    int    ok;                 // 1 = success, 0 = error
    double latitude;           // WGS-84 degrees
    double longitude;
    double altitude;           // metres (NaN if unavailable)
    double horizontal_accuracy;// metres  (-1 if unavailable)
    double vertical_accuracy;  // metres  (-1 if unavailable)
    double speed;              // m/s     (-1 if unavailable)
    double course;             // degrees (-1 if unavailable)
    double timestamp;          // unix seconds (double)
    int    auth_status;        // CLAuthorizationStatus raw value
    char   error[256];         // NUL-terminated error message (empty on success)
} SkillLocationResult;

// ── Auth status ──────────────────────────────────────────────────────────────

typedef enum {
    SkillLocAuthNotDetermined = 0,
    SkillLocAuthRestricted    = 1,
    SkillLocAuthDenied        = 2,
    SkillLocAuthAuthorized    = 3,
} SkillLocAuthStatus;

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
    // If denied or restricted, stop waiting
    if (self.authStatus == kCLAuthorizationStatusDenied ||
        self.authStatus == kCLAuthorizationStatusRestricted) {
        self.done = YES;
    }
}

@end

// ── Public C API ─────────────────────────────────────────────────────────────

int skill_location_auth_status(void) {
    __block CLAuthorizationStatus status;

    if ([NSThread isMainThread]) {
        CLLocationManager *mgr = [[CLLocationManager alloc] init];
        status = mgr.authorizationStatus;
    } else {
        dispatch_sync(dispatch_get_main_queue(), ^{
            CLLocationManager *mgr = [[CLLocationManager alloc] init];
            status = mgr.authorizationStatus;
        });
    }

    switch (status) {
        case kCLAuthorizationStatusNotDetermined:     return SkillLocAuthNotDetermined;
        case kCLAuthorizationStatusRestricted:        return SkillLocAuthRestricted;
        case kCLAuthorizationStatusDenied:            return SkillLocAuthDenied;
        case kCLAuthorizationStatusAuthorizedAlways:  return SkillLocAuthAuthorized;
        default:                                      return SkillLocAuthAuthorized;
    }
}

/// Request location permission.  Returns 1 if access is (or becomes)
/// authorized, 0 otherwise.  Blocks for up to `timeout_secs` seconds.
int skill_location_request_access(double timeout_secs) {
    __block int result = 0;

    void (^work)(void) = ^{
        CLLocationManager *mgr = [[CLLocationManager alloc] init];
        SkillLocationDelegate *del = [[SkillLocationDelegate alloc] init];
        del.authStatus = mgr.authorizationStatus;
        mgr.delegate = del;

        if (mgr.authorizationStatus == kCLAuthorizationStatusNotDetermined) {
            // On macOS, requestWhenInUseAuthorization triggers the system
            // dialog.  The delegate callback fires when the user responds.
            [mgr requestWhenInUseAuthorization];

            NSDate *deadline = [NSDate dateWithTimeIntervalSinceNow:timeout_secs];
            while (!del.done &&
                   del.authStatus == kCLAuthorizationStatusNotDetermined &&
                   [[NSDate date] compare:deadline] == NSOrderedAscending) {
                [[NSRunLoop currentRunLoop] runMode:NSDefaultRunLoopMode
                                         beforeDate:[NSDate dateWithTimeIntervalSinceNow:0.1]];
            }
        }

        result = (del.authStatus == kCLAuthorizationStatusAuthorizedAlways) ? 1 : 0;
    };

    if ([NSThread isMainThread]) {
        work();
    } else {
        dispatch_sync(dispatch_get_main_queue(), work);
    }

    return result;
}

/// Fetch the current location.  Blocks for up to `timeout_secs`.
void skill_location_fetch(double timeout_secs, SkillLocationResult *out) {
    memset(out, 0, sizeof(*out));
    out->altitude           = NAN;
    out->horizontal_accuracy = -1;
    out->vertical_accuracy   = -1;
    out->speed              = -1;
    out->course             = -1;

    __block SkillLocationResult local_result = *out;

    void (^work)(void) = ^{
        CLLocationManager *mgr = [[CLLocationManager alloc] init];
        SkillLocationDelegate *del = [[SkillLocationDelegate alloc] init];
        del.authStatus = mgr.authorizationStatus;
        mgr.delegate = del;
        mgr.desiredAccuracy = kCLLocationAccuracyBest;

        local_result.auth_status = (int)mgr.authorizationStatus;

        if (mgr.authorizationStatus != kCLAuthorizationStatusAuthorizedAlways) {
            local_result.ok = 0;
            snprintf(local_result.error, sizeof(local_result.error),
                     "location not authorized (status=%d)", (int)mgr.authorizationStatus);
            return;
        }

        [mgr requestLocation];

        NSDate *deadline = [NSDate dateWithTimeIntervalSinceNow:timeout_secs];
        while (!del.done &&
               [[NSDate date] compare:deadline] == NSOrderedAscending) {
            [[NSRunLoop currentRunLoop] runMode:NSDefaultRunLoopMode
                                     beforeDate:[NSDate dateWithTimeIntervalSinceNow:0.1]];
        }

        if (del.lastLocation) {
            CLLocation *loc = del.lastLocation;
            local_result.ok                  = 1;
            local_result.latitude            = loc.coordinate.latitude;
            local_result.longitude           = loc.coordinate.longitude;
            local_result.altitude            = loc.altitude;
            local_result.horizontal_accuracy = loc.horizontalAccuracy;
            local_result.vertical_accuracy   = loc.verticalAccuracy;
            local_result.speed               = loc.speed;
            local_result.course              = loc.course;
            local_result.timestamp           = [loc.timestamp timeIntervalSince1970];
        } else if (del.lastError) {
            local_result.ok = 0;
            snprintf(local_result.error, sizeof(local_result.error),
                     "%s", del.lastError.localizedDescription.UTF8String ?: "unknown error");
        } else {
            local_result.ok = 0;
            snprintf(local_result.error, sizeof(local_result.error), "location request timed out");
        }
    };

    if ([NSThread isMainThread]) {
        work();
    } else {
        dispatch_sync(dispatch_get_main_queue(), work);
    }

    *out = local_result;
}
