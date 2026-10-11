#import "mote_ios.h"
#import <UIKit/UIKit.h>

static UIWindow *MoteWindow;
static UIWindowScene *MoteWindowScene;

static void attachWindowToScene(void) {
    if (MoteWindow == nil || MoteWindowScene == nil) {
        return;
    }
    MoteWindow.windowScene = MoteWindowScene;
    MoteWindow.frame = MoteWindowScene.coordinateSpace.bounds;
    [MoteWindow makeKeyAndVisible];
}

void mote_ios_attach_view(void *view) {
    MoteWindow = ((__bridge UIView *)view).window;
    dispatch_async(dispatch_get_main_queue(), ^{ attachWindowToScene(); });
}

MoteInsets mote_ios_safe_area(void) {
    UIEdgeInsets insets = MoteWindow.safeAreaInsets;
    return (MoteInsets){insets.top, insets.left, insets.bottom, insets.right};
}

@interface MoteSceneDelegate : UIResponder <UIWindowSceneDelegate>
@end

@implementation MoteSceneDelegate
- (void)scene:(UIScene *)scene willConnectToSession:(UISceneSession *)session
      options:(UISceneConnectionOptions *)connectionOptions {
    if ([scene isKindOfClass:[UIWindowScene class]]) {
        MoteWindowScene = (UIWindowScene *)scene;
        attachWindowToScene();
    }
}

- (void)sceneDidDisconnect:(UIScene *)scene {
    if (scene == MoteWindowScene) {
        MoteWindowScene = nil;
    }
}
@end

int main(int argc, char *argv[]) {
    @autoreleasepool {
        mote_ios_main();
    }
}
