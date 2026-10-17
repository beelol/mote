#import "mote_ios.h"
#import <UIKit/UIKit.h>

uint8_t mote_ios_reduce_motion_enabled(void) {
    return UIAccessibilityIsReduceMotionEnabled() ? 1 : 0;
}

void *mote_ios_motion_observe(void (*changed)(void)) {
    if (!changed) return NULL;
    id observer = [NSNotificationCenter.defaultCenter
        addObserverForName:UIAccessibilityReduceMotionStatusDidChangeNotification
        object:nil queue:nil usingBlock:^(NSNotification *notification) {
            changed();
        }];
    return (__bridge_retained void *)observer;
}

void mote_ios_motion_remove(void *observer) {
    if (!observer) return;
    id token = CFBridgingRelease(observer);
    [NSNotificationCenter.defaultCenter removeObserver:token];
}
