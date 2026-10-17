#import "mote_ios.h"
#import <UIKit/UIKit.h>

static __weak UIView *MoteAppearanceRootView;

@interface MoteAppearanceObserverView : UIView
@property(nonatomic) void (*changed)(void);
@end

@implementation MoteAppearanceObserverView
- (void)didMoveToWindow {
    [super didMoveToWindow];
    if (self.window && self.changed) self.changed();
}

- (void)traitCollectionDidChange:(UITraitCollection *)previousTraitCollection {
    [super traitCollectionDidChange:previousTraitCollection];
    if (self.changed &&
        previousTraitCollection.userInterfaceStyle != self.traitCollection.userInterfaceStyle) {
        self.changed();
    }
}
@end

void mote_ios_appearance_attach(void *view) {
    MoteAppearanceRootView = (__bridge UIView *)view;
}

uint8_t mote_ios_appearance(void) {
    UIUserInterfaceStyle style = MoteAppearanceRootView.traitCollection.userInterfaceStyle;
    if (style == UIUserInterfaceStyleUnspecified) {
        style = UIScreen.mainScreen.traitCollection.userInterfaceStyle;
    }
    if (style == UIUserInterfaceStyleLight) return 1;
    if (style == UIUserInterfaceStyleDark) return 2;
    return 0;
}

void *mote_ios_appearance_observe(void (*changed)(void)) {
    if (!changed || !MoteAppearanceRootView) return NULL;
    MoteAppearanceObserverView *observer = [[MoteAppearanceObserverView alloc] initWithFrame:CGRectZero];
    observer.changed = changed;
    observer.userInteractionEnabled = NO;
    observer.isAccessibilityElement = NO;
    [MoteAppearanceRootView addSubview:observer];
    return (__bridge_retained void *)observer;
}

void mote_ios_appearance_remove(void *observer) {
    if (!observer) return;
    MoteAppearanceObserverView *view = CFBridgingRelease(observer);
    view.changed = NULL;
    [view removeFromSuperview];
}

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
