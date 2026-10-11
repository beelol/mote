#import <XCTest/XCTest.h>

@interface MoteUITests : XCTestCase
@end

@implementation MoteUITests
- (void)testOfflineGalleryLaunchRemainsInForeground {
    XCUIApplication *app = [[XCUIApplication alloc] init];
    [app launch];
    XCTAssertTrue([app waitForState:XCUIApplicationStateRunningForeground timeout:15]);
    XCTAssertTrue([app.windows.firstMatch waitForExistenceWithTimeout:15]);
    XCTestExpectation *survival = [self expectationWithDescription:@"App survives startup"];
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 5 * NSEC_PER_SEC), dispatch_get_main_queue(), ^{
        XCTAssertEqual(app.state, XCUIApplicationStateRunningForeground);
        XCTAttachment *capture = [XCTAttachment attachmentWithScreenshot:app.screenshot];
        capture.name = @"Offline gallery";
        capture.lifetime = XCTAttachmentLifetimeKeepAlways;
        [self addAttachment:capture];
        [survival fulfill];
    });
    [self waitForExpectations:@[survival] timeout:10];
}
@end
