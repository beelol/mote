#import <XCTest/XCTest.h>
@import Vision;

@interface MoteUITests : XCTestCase
@end

@implementation MoteUITests
- (void)setUp {
    [super setUp];
    self.continueAfterFailure = NO;
}
- (NSArray<VNRecognizedTextObservation *> *)recognize:(XCUIApplication *)app {
    VNRecognizeTextRequest *request = [[VNRecognizeTextRequest alloc] init];
    request.recognitionLevel = VNRequestTextRecognitionLevelAccurate;
    request.usesLanguageCorrection = NO;
    UIImage *source = app.windows.firstMatch.screenshot.image;
    CGSize size = app.windows.firstMatch.frame.size;
    UIGraphicsImageRenderer *renderer = [[UIGraphicsImageRenderer alloc] initWithSize:size];
    UIImage *image = [renderer imageWithActions:^(UIGraphicsImageRendererContext *context) {
        [source drawInRect:CGRectMake(0, 0, size.width, size.height)];
    }];
    VNImageRequestHandler *handler = [[VNImageRequestHandler alloc] initWithCGImage:image.CGImage options:@{}];
    NSError *error = nil;
    XCTAssertTrue([handler performRequests:@[request] error:&error], @"%@", error);
    return request.results;
}

- (NSString *)visibleText:(XCUIApplication *)app {
    NSMutableArray *lines = [NSMutableArray array];
    for (VNRecognizedTextObservation *item in [self recognize:app]) {
        [lines addObject:[item topCandidates:1].firstObject.string];
    }
    return [lines componentsJoinedByString:@" "];
}

- (void)assertText:(NSString *)text app:(XCUIApplication *)app {
    NSString *expected = [[text componentsSeparatedByCharactersInSet:NSCharacterSet.whitespaceAndNewlineCharacterSet] componentsJoinedByString:@""];
    NSPredicate *predicate = [NSPredicate predicateWithBlock:^BOOL(id object, NSDictionary *bindings) {
        NSString *visible = [[[self visibleText:app] componentsSeparatedByCharactersInSet:NSCharacterSet.whitespaceAndNewlineCharacterSet] componentsJoinedByString:@""];
        return [visible containsString:expected];
    }];
    XCTNSPredicateExpectation *expectation = [[XCTNSPredicateExpectation alloc] initWithPredicate:predicate object:app];
    XCTAssertEqual([XCTWaiter waitForExpectations:@[expectation] timeout:8], XCTWaiterResultCompleted,
                   @"Missing %@ in %@", text, [self visibleText:app]);
}

- (void)capture:(NSString *)name app:(XCUIApplication *)app {
    XCTAttachment *capture = [XCTAttachment attachmentWithScreenshot:app.screenshot];
    capture.name = name;
    capture.lifetime = XCTAttachmentLifetimeKeepAlways;
    [self addAttachment:capture];
}

- (void)tapText:(NSString *)text app:(XCUIApplication *)app {
    NSArray *items = [self recognize:app];
    for (NSInteger exact = 1; exact >= 0; exact--) {
      for (VNRecognizedTextObservation *item in items) {
        NSString *line = [item topCandidates:1].firstObject.string;
        if (exact ? [line isEqualToString:text] : [line containsString:text]) {
            CGRect box = item.boundingBox;
            CGVector normalized = CGVectorMake(CGRectGetMidX(box), 1.0 - CGRectGetMidY(box));
            NSLog(@"MOTE TAP %@ point=(%.2f, %.2f)", text, normalized.dx * app.windows.firstMatch.frame.size.width, normalized.dy * app.windows.firstMatch.frame.size.height);
            [[app.windows.firstMatch coordinateWithNormalizedOffset:normalized] tap];
            return;
        }
      }
    }
    XCTFail(@"Cannot locate %@ in %@", text, [self visibleText:app]);
}

- (void)moveSlider:(XCUIApplication *)app {
    for (VNRecognizedTextObservation *item in [self recognize:app]) {
        if ([[item topCandidates:1].firstObject.string isEqualToString:@"Volume"]) {
            CGRect window = app.windows.firstMatch.frame;
            CGPoint point = CGPointMake(CGRectGetMinX(item.boundingBox) * window.size.width + 20.0,
                                        (1.0 - CGRectGetMinY(item.boundingBox)) * window.size.height + 16.0);
            NSLog(@"MOTE SLIDER point=(%.2f, %.2f)", point.x, point.y);
            [[[app.windows.firstMatch coordinateWithNormalizedOffset:CGVectorMake(0, 0)] coordinateWithOffset:CGVectorMake(point.x, point.y)] tap];
            return;
        }
    }
    XCTFail(@"Volume slider is missing");
}

- (void)testGalleryPagesAndControlsInBothOrientations {
    XCUIApplication *app = [[XCUIApplication alloc] init];
    [app launch];
    XCTAssertTrue([app waitForState:XCUIApplicationStateRunningForeground timeout:15]);
    for (NSNumber *orientation in @[@(UIDeviceOrientationPortrait), @(UIDeviceOrientationLandscapeLeft)]) {
        [app terminate];
        XCUIDevice.sharedDevice.orientation = orientation.integerValue;
        [app launch];
        XCTAssertTrue([app waitForState:XCUIApplicationStateRunningForeground timeout:15]);
        NSArray *pages = @[@"Layout", @"Toggle", @"Slider", @"Select", @"List", @"Form", @"Dashboard", @"Fixture"];
        for (NSString *page in pages) {
            [self tapText:page app:app];
            [self assertText:[@"Selected " stringByAppendingString:page] app:app];
            [self capture:[NSString stringWithFormat:@"%@-%@", orientation, page] app:app];
        }
        [self tapText:@"Layout" app:app];
        [self tapText:@"Preview" app:app];
        [self assertText:@"Action: gallery.layout" app:app];
        [self tapText:@"Toggle" app:app];
        [self tapText:@"Enable preview" app:app];
        [self assertText:@"Changed gallery_enabled: false" app:app];
        [self tapText:@"Select" app:app];
        [self tapText:@"Priority:" app:app];
        [self tapText:@"High" app:app];
        [self assertText:@"Changed gallery_priority:" app:app];
        [self assertText:@"Priority: High" app:app];
        [self tapText:@"Slider" app:app];
        [self moveSlider:app];
        [self assertText:@"Changed gallery_volume:" app:app];
        [self tapText:@"List" app:app];
        [self assertText:@"Open draft" app:app];
        [self capture:[NSString stringWithFormat:@"%@-list-before-scroll", orientation] app:app];
        for (NSInteger swipe = 0; swipe < 5; swipe++) {
            XCUICoordinate *start = [app.windows.firstMatch coordinateWithNormalizedOffset:CGVectorMake(0.85, 0.7)];
            XCUICoordinate *end = [app.windows.firstMatch coordinateWithNormalizedOffset:CGVectorMake(0.85, 0.25)];
            [start pressForDuration:0.05 thenDragToCoordinate:end];
        }
        [self assertText:@"Open item 20" app:app];
        [self tapText:@"Open item 20" app:app];
        [self assertText:@"Action: gallery.item.20" app:app];
        [self capture:[NSString stringWithFormat:@"%@-list-after-scroll", orientation] app:app];
        [self tapText:@"Form" app:app];
        [self tapText:@"Enable preview" app:app];
        [self assertText:@"Changed gallery_form_enabled: false" app:app];
        [self tapText:@"Priority:" app:app];
        [self tapText:@"High" app:app];
        [self assertText:@"Changed gallery_form_priority:" app:app];
        [self moveSlider:app];
        [self assertText:@"Changed gallery_form_volume:" app:app];
        [self tapText:@"Submit preview" app:app];
        [self assertText:@"Action: gallery.submit" app:app];
        [self capture:[NSString stringWithFormat:@"%@-form-submitted", orientation] app:app];
        [self tapText:@"Dashboard" app:app];
        [self tapText:@"Local only" app:app];
        [self assertText:@"Changed models.local_only: false" app:app];
        for (NSArray *model in @[@[@"Talk model:", @"models.talk", @"qwen3:0.6b"], @[@"Router model:", @"models.router", @"qwen3:4b"], @[@"Think model:", @"models.think", @"qwen3:4b"]]) {
            [self tapText:model[0] app:app];
            [self tapText:model[2] app:app];
            [self assertText:[@"Changed " stringByAppendingString:model[1]] app:app];
        }
        [self tapText:@"Refresh" app:app];
        [self assertText:@"Action: refresh" app:app];
        [self tapText:@"Fixture" app:app];
        [self tapText:@"Local only" app:app];
        [self assertText:@"Changed models.local_only: false" app:app];
        [self tapText:@"Mode:" app:app];
        [self tapText:@"quiet" app:app];
        [self assertText:@"Changed mode:" app:app];
        [self moveSlider:app];
        [self assertText:@"Changed volume:" app:app];
        [self tapText:@"Refresh" app:app];
        [self assertText:@"Action: refresh" app:app];
        [self tapText:@"Enter a preview note" app:app];
        XCTAssertTrue([app.keyboards.firstMatch waitForExistenceWithTimeout:5]);
        [app.textViews.firstMatch typeText:@"iPad preview\n"];
        [self assertText:@"iPad preview" app:app];
        [self capture:[NSString stringWithFormat:@"%@-text-entry", orientation] app:app];
        [self tapText:@"Layout" app:app];
        [self tapText:@"Preview" app:app];
    }
    XCUIDevice.sharedDevice.orientation = UIDeviceOrientationPortrait;
}
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

- (void)testDeterministicFixtureAndReducedMotionLaunch {
    XCUIApplication *app = [[XCUIApplication alloc] init];
    app.launchArguments = @[@"--mote-fixture", @"slider-layout", @"--mote-reduced-motion"];
    [app launch];
    XCTAssertTrue([app waitForState:XCUIApplicationStateRunningForeground timeout:15]);
    [self assertText:@"Mode: normal" app:app];
    [self capture:@"reduced-motion-fixture" app:app];
    [self tapText:@"Toggle" app:app];
    [self assertText:@"Selected Toggle" app:app];
    [self capture:@"reduced-motion-toggle" app:app];
}
@end
