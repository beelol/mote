#import <XCTest/XCTest.h>
@import Vision;

static NSString *compactText(NSString *text) {
    return [[text componentsSeparatedByCharactersInSet:NSCharacterSet.whitespaceAndNewlineCharacterSet] componentsJoinedByString:@""];
}

static BOOL statusTextMatches(NSString *observed, NSString *expected) {
    return [compactText(observed) isEqualToString:compactText(expected)];
}

static NSString *textInRow(NSArray<NSDictionary *> *fragments, NSString *prefix, CGFloat height) {
    NSDictionary *anchor = nil;
    for (NSDictionary *fragment in fragments) {
        if ([compactText(fragment[@"text"]) containsString:compactText(prefix)]) {
            anchor = fragment;
            break;
        }
    }
    if (!anchor) return @"";
    CGFloat center = CGRectGetMidY([anchor[@"box"] CGRectValue]);
    NSMutableArray *row = [NSMutableArray array];
    for (NSDictionary *fragment in fragments) {
        if (fabs(CGRectGetMidY([fragment[@"box"] CGRectValue]) - center) * height <= 4) {
            [row addObject:fragment];
        }
    }
    [row sortUsingComparator:^NSComparisonResult(NSDictionary *left, NSDictionary *right) {
        CGFloat x1 = CGRectGetMinX([left[@"box"] CGRectValue]);
        CGFloat x2 = CGRectGetMinX([right[@"box"] CGRectValue]);
        return x1 < x2 ? NSOrderedAscending : x1 > x2 ? NSOrderedDescending : NSOrderedSame;
    }];
    NSMutableArray *texts = [NSMutableArray array];
    for (NSDictionary *fragment in row) {
        [texts addObject:fragment[@"text"]];
    }
    return [texts componentsJoinedByString:@" "];
}

@interface MoteUITests : XCTestCase
@property(nonatomic) NSUInteger recognitionMisses;
@end

@implementation MoteUITests
- (void)setUp {
    [super setUp];
    self.continueAfterFailure = NO;
}
- (void)tearDown {
    self.recognitionMisses = 0;
    XCUIDevice.sharedDevice.orientation = UIDeviceOrientationPortrait;
    [super tearDown];
}
- (UIImage *)windowImage:(XCUIApplication *)app {
    UIImage *source = app.windows.firstMatch.screenshot.image;
    CGSize size = app.windows.firstMatch.frame.size;
    UIGraphicsImageRenderer *renderer = [[UIGraphicsImageRenderer alloc] initWithSize:size];
    return [renderer imageWithActions:^(UIGraphicsImageRendererContext *context) {
        [source drawInRect:CGRectMake(0, 0, size.width, size.height)];
    }];
}

- (NSArray<VNRecognizedTextObservation *> *)recognize:(XCUIApplication *)app {
    if (self.recognitionMisses > 0) {
        self.recognitionMisses--;
        return @[];
    }
    VNRecognizeTextRequest *request = [[VNRecognizeTextRequest alloc] init];
    request.recognitionLevel = VNRequestTextRecognitionLevelAccurate;
    request.usesLanguageCorrection = NO;
    UIImage *image = [self windowImage:app];
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
    NSString *expected = compactText(text);
    NSString *prefix = nil;
    if ([text hasPrefix:@"Changed "] || [text hasPrefix:@"Action:"]) {
        prefix = [text substringToIndex:[text rangeOfString:@":"].location + 1];
    } else if ([text hasPrefix:@"Selected "]) {
        prefix = @"Selected ";
    }
    NSPredicate *predicate = [NSPredicate predicateWithBlock:^BOOL(id object, NSDictionary *bindings) {
        NSString *visible = nil;
        if (prefix) {
            NSMutableArray *fragments = [NSMutableArray array];
            for (VNRecognizedTextObservation *item in [self recognize:app]) {
                [fragments addObject:@{@"text": [item topCandidates:1].firstObject.string,
                                       @"box": [NSValue valueWithCGRect:item.boundingBox]}];
            }
            visible = textInRow(fragments, prefix, app.windows.firstMatch.frame.size.height);
        } else {
            visible = [self visibleText:app];
        }
        return prefix ? statusTextMatches(visible, text) : [compactText(visible) containsString:expected];
    }];
    XCTNSPredicateExpectation *expectation = [[XCTNSPredicateExpectation alloc] initWithPredicate:predicate object:app];
    XCTAssertEqual([XCTWaiter waitForExpectations:@[expectation] timeout:8], XCTWaiterResultCompleted,
                   @"Missing %@ in %@", text, [self visibleText:app]);
}

- (void)assertControlText:(NSString *)text label:(NSString *)label app:(XCUIApplication *)app {
    NSString *expected = [[text componentsSeparatedByCharactersInSet:NSCharacterSet.whitespaceAndNewlineCharacterSet] componentsJoinedByString:@""];
    __block NSString *observed = @"";
    NSPredicate *predicate = [NSPredicate predicateWithBlock:^BOOL(id object, NSDictionary *bindings) {
        for (VNRecognizedTextObservation *item in [self recognize:app]) {
            if (![[item topCandidates:1].firstObject.string containsString:label]) continue;
            UIImage *source = [self windowImage:app];
            CGFloat width = CGImageGetWidth(source.CGImage);
            CGFloat height = CGImageGetHeight(source.CGImage);
            CGFloat scale = width / source.size.width;
            CGRect box = item.boundingBox;
            CGRect rect = CGRectMake(box.origin.x * width, (1 - CGRectGetMaxY(box)) * height,
                                     box.size.width * width, box.size.height * height);
            rect = CGRectIntersection(CGRectInset(rect, -8 * scale, -6 * scale), CGRectMake(0, 0, width, height));
            CGImageRef cropped = CGImageCreateWithImageInRect(source.CGImage, rect);
            UIImage *crop = [UIImage imageWithCGImage:cropped];
            CGImageRelease(cropped);
            UIGraphicsImageRendererFormat *format = UIGraphicsImageRendererFormat.defaultFormat;
            format.scale = 1;
            UIGraphicsImageRenderer *renderer = [[UIGraphicsImageRenderer alloc] initWithSize:CGSizeMake(crop.size.width * 3, crop.size.height * 3) format:format];
            UIImage *image = [renderer imageWithActions:^(UIGraphicsImageRendererContext *context) {
                [crop drawInRect:CGRectMake(0, 0, crop.size.width * 3, crop.size.height * 3)];
            }];
            VNRecognizeTextRequest *request = [[VNRecognizeTextRequest alloc] init];
            request.recognitionLevel = VNRequestTextRecognitionLevelAccurate;
            request.usesLanguageCorrection = NO;
            VNImageRequestHandler *handler = [[VNImageRequestHandler alloc] initWithCGImage:image.CGImage options:@{}];
            NSError *error = nil;
            XCTAssertTrue([handler performRequests:@[request] error:&error], @"%@", error);
            NSMutableArray *lines = [NSMutableArray array];
            for (VNRecognizedTextObservation *line in request.results) {
                [lines addObject:[line topCandidates:1].firstObject.string];
            }
            observed = [lines componentsJoinedByString:@" "];
            NSString *compact = [[observed componentsSeparatedByCharactersInSet:NSCharacterSet.whitespaceAndNewlineCharacterSet] componentsJoinedByString:@""];
            if ([compact containsString:expected]) return YES;
        }
        return NO;
    }];
    XCTNSPredicateExpectation *expectation = [[XCTNSPredicateExpectation alloc] initWithPredicate:predicate object:app];
    XCTAssertEqual([XCTWaiter waitForExpectations:@[expectation] timeout:8], XCTWaiterResultCompleted,
                   @"Missing exact control %@ in crop %@", text, observed);
}

- (void)capture:(NSString *)name app:(XCUIApplication *)app {
    XCTAttachment *capture = [XCTAttachment attachmentWithScreenshot:app.screenshot];
    capture.name = name;
    capture.lifetime = XCTAttachmentLifetimeKeepAlways;
    [self addAttachment:capture];
}

- (VNRecognizedTextObservation *)findText:(NSString *)text app:(XCUIApplication *)app {
    __block VNRecognizedTextObservation *found = nil;
    NSPredicate *predicate = [NSPredicate predicateWithBlock:^BOOL(id object, NSDictionary *bindings) {
        NSArray *items = [self recognize:app];
        for (NSInteger exact = 1; exact >= 0; exact--) {
            for (VNRecognizedTextObservation *item in items) {
                NSString *line = [item topCandidates:1].firstObject.string;
                if (exact ? [line isEqualToString:text] : [line containsString:text]) {
                    found = item;
                    return YES;
                }
            }
        }
        return NO;
    }];
    XCTNSPredicateExpectation *expectation = [[XCTNSPredicateExpectation alloc] initWithPredicate:predicate object:app];
    XCTAssertEqual([XCTWaiter waitForExpectations:@[expectation] timeout:8], XCTWaiterResultCompleted,
                   @"Cannot locate %@ in %@", text, [self visibleText:app]);
    return found;
}

- (void)tapText:(NSString *)text app:(XCUIApplication *)app {
    CGRect box = [self findText:text app:app].boundingBox;
    CGVector normalized = CGVectorMake(CGRectGetMidX(box), 1.0 - CGRectGetMidY(box));
    NSLog(@"MOTE TAP %@ point=(%.2f, %.2f)", text, normalized.dx * app.windows.firstMatch.frame.size.width, normalized.dy * app.windows.firstMatch.frame.size.height);
    [[app.windows.firstMatch coordinateWithNormalizedOffset:normalized] tap];
}

- (CGRect)moveSlider:(XCUIApplication *)app {
    VNRecognizedTextObservation *item = [self findText:@"Volume" app:app];
    CGRect window = app.windows.firstMatch.frame;
    CGPoint point = CGPointMake(CGRectGetMinX(item.boundingBox) * window.size.width + 4.0,
                                (1.0 - CGRectGetMinY(item.boundingBox)) * window.size.height + 16.0);
    NSLog(@"MOTE SLIDER point=(%.2f, %.2f)", point.x, point.y);
    [[[app.windows.firstMatch coordinateWithNormalizedOffset:CGVectorMake(0, 0)] coordinateWithOffset:CGVectorMake(point.x, point.y)] tap];
    return item.boundingBox;
}

- (void)assertSliderThumbAtMinimum:(BOOL)expected label:(CGRect)label app:(XCUIApplication *)app {
    UIImage *image = [self windowImage:app];
    size_t width = CGImageGetWidth(image.CGImage);
    size_t height = CGImageGetHeight(image.CGImage);
    CGFloat scale = width / image.size.width;
    NSMutableData *pixels = [NSMutableData dataWithLength:width * height * 4];
    CGColorSpaceRef colors = CGColorSpaceCreateDeviceRGB();
    CGContextRef context = CGBitmapContextCreate(pixels.mutableBytes, width, height, 8,
                                                 width * 4, colors, kCGImageAlphaPremultipliedLast | kCGBitmapByteOrder32Big);
    XCTAssertNotEqual(context, NULL);
    CGContextDrawImage(context, CGRectMake(0, 0, width, height), image.CGImage);
    CGContextRelease(context);
    CGColorSpaceRelease(colors);
    const unsigned char *bytes = pixels.bytes;
    CGFloat left = CGRectGetMinX(label) * width;
    CGFloat bottom = (1.0 - CGRectGetMinY(label)) * height;
    NSUInteger redPixels = 0;
    for (size_t y = bottom + 6 * scale; y < MIN(height, bottom + 11 * scale); y++) {
        for (size_t x = left + 4 * scale; x < MIN(width, left + 16 * scale); x++) {
            const unsigned char *color = bytes + (y * width + x) * 4;
            if (color[0] > 60 && color[0] > color[1] * 2 && color[0] > color[2] * 2) {
                redPixels++;
            }
        }
    }
    NSLog(@"MOTE SLIDER MINIMUM redPixels=%lu scale=%.2f", (unsigned long)redPixels, scale);
    if (expected) {
        XCTAssertGreaterThanOrEqual(redPixels, (NSUInteger)(3 * scale * scale), @"Minimum-position thumb is not visible");
    } else {
        XCTAssertLessThan(redPixels, (NSUInteger)(3 * scale * scale), @"Unedited slider appears at its minimum");
    }
}

- (void)testGalleryPagesAndControlsInBothOrientations {
    XCUIApplication *app = [[XCUIApplication alloc] init];
    app.launchArguments = @[@"--mote-offline"];
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
        [self assertText:@"Changed gallery_priority: \"High\"" app:app];
        [self assertControlText:@"Priority: High" label:@"Priority:" app:app];
        [self tapText:@"Slider" app:app];
        CGRect sliderLabel = [self moveSlider:app];
        [self assertText:@"Changed gallery_volume: 0.0" app:app];
        [self assertSliderThumbAtMinimum:YES label:sliderLabel app:app];
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
        [self assertText:@"Changed gallery_form_priority: \"High\"" app:app];
        [self assertControlText:@"Priority: High" label:@"Priority:" app:app];
        sliderLabel = [self moveSlider:app];
        [self assertText:@"Changed gallery_form_volume: 0.0" app:app];
        [self assertSliderThumbAtMinimum:YES label:sliderLabel app:app];
        [self tapText:@"Submit preview" app:app];
        [self assertText:@"Action: gallery.submit" app:app];
        [self capture:[NSString stringWithFormat:@"%@-form-submitted", orientation] app:app];
        [self tapText:@"Dashboard" app:app];
        [self tapText:@"Local only" app:app];
        [self assertText:@"Changed models.local_only: false" app:app];
        for (NSArray *model in @[@[@"Talk model:", @"models.talk", @"qwen3:0.6b"], @[@"Router model:", @"models.router", @"qwen3:4b"], @[@"Think model:", @"models.think", @"qwen3:4b"]]) {
            [self tapText:model[0] app:app];
            [self tapText:model[2] app:app];
            [self assertText:[NSString stringWithFormat:@"Changed %@: \"%@\"", model[1], model[2]] app:app];
            [self assertControlText:[NSString stringWithFormat:@"%@ %@", model[0], model[2]] label:model[0] app:app];
        }
        [self tapText:@"Refresh" app:app];
        [self assertText:@"Action: refresh" app:app];
        [self tapText:@"Fixture" app:app];
        [self tapText:@"Local only" app:app];
        [self assertText:@"Changed models.local_only: false" app:app];
        [self tapText:@"Mode:" app:app];
        [self tapText:@"quiet" app:app];
        [self assertText:@"Changed mode: \"quiet\"" app:app];
        [self assertControlText:@"Mode: quiet" label:@"Mode:" app:app];
        sliderLabel = [self moveSlider:app];
        [self assertText:@"Changed volume: 0.0" app:app];
        [self assertSliderThumbAtMinimum:YES label:sliderLabel app:app];
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
    app.launchArguments = @[@"--mote-offline"];
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
    app.launchArguments = @[@"--mote-offline", @"--mote-fixture", @"slider-layout", @"--mote-reduced-motion"];
    [app launch];
    XCTAssertTrue([app waitForState:XCUIApplicationStateRunningForeground timeout:15]);
    [self assertText:@"Mode: normal" app:app];
    [self capture:@"reduced-motion-fixture" app:app];
    [self tapText:@"Toggle" app:app];
    [self assertText:@"Selected Toggle" app:app];
    [self capture:@"reduced-motion-toggle" app:app];
}

- (void)testTextTargetRetriesTransientRecognitionMisses {
    XCUIApplication *app = [[XCUIApplication alloc] init];
    app.launchArguments = @[@"--mote-offline"];
    [app launch];
    XCTAssertTrue([app waitForState:XCUIApplicationStateRunningForeground timeout:15]);
    self.recognitionMisses = 2;
    [self tapText:@"Toggle" app:app];
    XCTAssertEqual(self.recognitionMisses, 0u);
    [self assertText:@"Selected Toggle" app:app];
}

- (void)testFixtureSliderReachesExactMinimum {
    XCUIApplication *app = [[XCUIApplication alloc] init];
    app.launchArguments = @[@"--mote-offline", @"--mote-fixture", @"slider-layout"];
    [app launch];
    XCTAssertTrue([app waitForState:XCUIApplicationStateRunningForeground timeout:15]);
    CGRect sliderLabel = [self findText:@"Volume" app:app].boundingBox;
    [self assertSliderThumbAtMinimum:NO label:sliderLabel app:app];
    sliderLabel = [self moveSlider:app];
    [self assertText:@"Changed volume: 0.0" app:app];
    [self assertSliderThumbAtMinimum:YES label:sliderLabel app:app];
    [self capture:@"fixture-exact-minimum" app:app];
}

- (void)testDashboardSelectValueVisibleInLandscape {
    XCUIDevice.sharedDevice.orientation = UIDeviceOrientationLandscapeLeft;
    XCUIApplication *app = [[XCUIApplication alloc] init];
    app.launchArguments = @[@"--mote-offline", @"--mote-page", @"dashboard"];
    [app launch];
    XCTAssertTrue([app waitForState:XCUIApplicationStateRunningForeground timeout:15]);
    [self tapText:@"Talk model:" app:app];
    [self tapText:@"qwen3:0.6b" app:app];
    [self assertText:@"Changed models.talk: \"qwen3:0.6b\"" app:app];
    [self capture:@"landscape-exact-talk" app:app];
    [self assertControlText:@"Talk model: qwen3:0.6b" label:@"Talk model:" app:app];
    [self tapText:@"Talk model:" app:app];
    [self tapText:@"qwen3:4b" app:app];
    [self assertText:@"Changed models.talk: \"qwen3:4b\"" app:app];
    [self assertControlText:@"Talk model: qwen3:4b" label:@"Talk model:" app:app];
    XCUIDevice.sharedDevice.orientation = UIDeviceOrientationPortrait;
}

- (void)testStatusRowOrdersSplitExactEventFragments {
    NSArray *fragments = @[
        @{@"text": @"Slider", @"box": [NSValue valueWithCGRect:CGRectMake(0.4, 0.88, 0.1, 0.02)]},
        @{@"text": @"\"qwen3:4b\"", @"box": [NSValue valueWithCGRect:CGRectMake(0.23, 0.8, 0.15, 0.02)]},
        @{@"text": @"Changed models.think:", @"box": [NSValue valueWithCGRect:CGRectMake(0.01, 0.801, 0.2, 0.02)]},
        @{@"text": @"Preview note", @"box": [NSValue valueWithCGRect:CGRectMake(0.01, 0.7, 0.15, 0.02)]}
    ];
    XCTAssertEqualObjects(textInRow(fragments, @"Changed models.think:", 1000), @"Changed models.think: \"qwen3:4b\"");
    XCTAssertEqualObjects(textInRow(fragments, @"missing", 1000), @"");
}

- (void)testExactStatusRejectsLongerNumericTokens {
    XCTAssertTrue(statusTextMatches(@"Changed volume: 0.0", @"Changed volume: 0.0"));
    XCTAssertTrue(statusTextMatches(@"Changed volume: 0.0 \n", @"Changed volume: 0.0"));
    XCTAssertFalse(statusTextMatches(@"Changed volume: 0.01", @"Changed volume: 0.0"));
    XCTAssertFalse(statusTextMatches(@"Changed volume: 0.0 extra", @"Changed volume: 0.0"));
}

- (void)testOptInTimingProbeInBothOrientations {
    XCUIApplication *app = [[XCUIApplication alloc] init];
    for (NSNumber *orientation in @[@(UIDeviceOrientationPortrait), @(UIDeviceOrientationLandscapeLeft)]) {
        for (NSNumber *reduced in @[@NO, @YES]) {
            [app terminate];
            XCUIDevice.sharedDevice.orientation = orientation.integerValue;
            app.launchArguments = reduced.boolValue ? @[@"--mote-offline", @"--mote-timing-probe", @"--mote-reduced-motion"] : @[@"--mote-offline", @"--mote-timing-probe"];
            [app launch];
            XCTAssertTrue([app waitForState:XCUIApplicationStateRunningForeground timeout:15]);
            NSTimeInterval start = NSDate.timeIntervalSinceReferenceDate;
            NSPredicate *elapsed = [NSPredicate predicateWithBlock:^BOOL(id object, NSDictionary *bindings) {
                return NSDate.timeIntervalSinceReferenceDate - start >= 18;
            }];
            XCTNSPredicateExpectation *wait = [[XCTNSPredicateExpectation alloc] initWithPredicate:elapsed object:nil];
            XCTAssertEqual([XCTWaiter waitForExpectations:@[wait] timeout:21], XCTWaiterResultCompleted);
            [self assertText:@"Timing probe complete" app:app];
            [self capture:[NSString stringWithFormat:@"timing-%@-%@", orientation, reduced] app:app];
        }
    }
}

- (void)testLivePairedHostsChatDashboardAndRelaunch {
    XCUIApplication *app = [[XCUIApplication alloc] init];
    [app activate];
    XCTAssertTrue([app waitForState:XCUIApplicationStateRunningForeground timeout:15]);
    [self assertText:@"Home: online" app:app];
    [self assertText:@"Home dashboard" app:app];
    [self capture:@"live-home-paired" app:app];
    [self tapText:@"Talk model:" app:app];
    [self tapText:@"qwen3:0.6b" app:app];
    [self assertText:@"Host accepted interaction" app:app];
    [self assertControlText:@"Talk model: qwen3:0.6b" label:@"Talk model:" app:app];
    [self capture:@"live-typed-dashboard-edit" app:app];
    [self tapText:@"Message your Mote host" app:app];
    XCTAssertTrue([app.keyboards.firstMatch waitForExistenceWithTimeout:5]);
    [app.textViews.firstMatch typeText:@"hello"];
    [self tapText:@"Send" app:app];
    [self assertText:@"User: hello" app:app];
    [self assertText:@"greeting" app:app];
    [self capture:@"live-chat" app:app];
    [app terminate];
    [app launch];
    [self assertText:@"Home: online" app:app];
    [self assertText:@"User: hello" app:app];
    [self assertControlText:@"Talk model: qwen3:0.6b" label:@"Talk model:" app:app];
    [self capture:@"live-relaunched-cursor" app:app];
    [self tapText:@"Office" app:app];
    [self assertText:@"Office: online" app:app];
    [self assertText:@"Office dashboard" app:app];
    XCTAssertFalse([compactText([self visibleText:app]) containsString:compactText(@"User: hello")]);
    [self capture:@"live-office-isolated" app:app];
    [self tapText:@"Home" app:app];
    [self assertText:@"Home dashboard" app:app];
    [self assertText:@"User: hello" app:app];
    XCTAssertFalse([compactText([self visibleText:app]) containsString:compactText(@"Office dashboard")]);
    [self capture:@"live-home-restored" app:app];
}

- (void)sendInferenceMessage:(NSString *)message prose:(NSString *)prose app:(XCUIApplication *)app {
    [self tapText:@"Message your Mote host" app:app];
    XCTAssertTrue([app.keyboards.firstMatch waitForExistenceWithTimeout:5]);
    [app.textViews.firstMatch typeText:message];
    [self tapText:@"Send" app:app];
    [self assertText:prose app:app];
}

- (void)testInferredSurfaceSuccessRetryPreservationAndFallback {
    XCUIApplication *app = [[XCUIApplication alloc] init];
    [app activate];
    [self assertText:@"Inference: online" app:app];
    [self assertText:@"Prior panel" app:app];
    [self sendInferenceMessage:@"Show a success panel" prose:@"Success prose." app:app];
    [self assertText:@"Success panel" app:app];
    [self capture:@"inferred-success" app:app];
    [self tapText:@"Refresh" app:app];
    [self assertText:@"Host accepted interaction" app:app];
    [self sendInferenceMessage:@"Recover a panel" prose:@"Recovery prose." app:app];
    [self assertText:@"Recovered panel" app:app];
    [self capture:@"inferred-one-retry" app:app];
    [self sendInferenceMessage:@"Keep my prior panel" prose:@"Preservation prose." app:app];
    [self assertText:@"Matrix source rejected" app:app];
    [self assertText:@"E_RESOLVE_ACTION" app:app];
    [self assertText:@"Recovered panel" app:app];
    XCTAssertFalse([[self visibleText:app] containsString:@"Invented"]);
    [self capture:@"inferred-prior-preserved" app:app];
    [self sendInferenceMessage:@"Answer with ordinary prose" prose:@"Ordinary prose fallback." app:app];
    [self assertText:@"Recovered panel" app:app];
    [self capture:@"inferred-prose-fallback" app:app];
}

- (void)assertExactLine:(NSString *)text timeout:(NSTimeInterval)timeout app:(XCUIApplication *)app {
    NSPredicate *predicate = [NSPredicate predicateWithBlock:^BOOL(id object, NSDictionary *bindings) {
        for (VNRecognizedTextObservation *item in [self recognize:app]) {
            if (statusTextMatches([item topCandidates:1].firstObject.string, text)) return YES;
        }
        return NO;
    }];
    XCTNSPredicateExpectation *expectation = [[XCTNSPredicateExpectation alloc] initWithPredicate:predicate object:app];
    XCTAssertEqual([XCTWaiter waitForExpectations:@[expectation] timeout:timeout], XCTWaiterResultCompleted,
                   @"Missing terminal row %@ in %@", text, [self visibleText:app]);
}

- (void)testBrowserApprovalDenyApproveRepeatAndNaturalExpiry {
    XCUIApplication *app = [[XCUIApplication alloc] init];
    [app activate];
    [self assertText:@"Browser: online" app:app];
    [self assertText:@"Browser fixture ready" app:app];
    NSLog(@"MOTE BROWSER READY");
    [self assertText:@"Pending" app:app];
    [self capture:@"browser-pending-deny" app:app];
    [app terminate];
    [app launch];
    [self assertText:@"Browser: online" app:app];
    [self assertText:@"Pending" app:app];
    [self capture:@"browser-pending-reconnected" app:app];
    [self tapText:@"Deny" app:app];
    [self assertExactLine:@"Denied" timeout:20 app:app];
    [self capture:@"browser-denied" app:app];
    NSLog(@"MOTE BROWSER DENIED OBSERVED");
    [self assertExactLine:@"Pending" timeout:20 app:app];
    [self capture:@"browser-pending-approve" app:app];
    CGRect box = [self findText:@"Approve" app:app].boundingBox;
    CGVector normalized = CGVectorMake(CGRectGetMidX(box), 1.0 - CGRectGetMidY(box));
    NSLog(@"MOTE BROWSER REPEAT TAP point=(%.2f, %.2f)", normalized.dx * app.windows.firstMatch.frame.size.width, normalized.dy * app.windows.firstMatch.frame.size.height);
    XCUICoordinate *approval = [app.windows.firstMatch coordinateWithNormalizedOffset:normalized];
    [approval tap];
    [approval tap];
    [self assertExactLine:@"Approved" timeout:20 app:app];
    [self assertText:@"Approved request completed." app:app];
    [self assertText:@"HTTP 200" app:app];
    [self capture:@"browser-approved-matrix-result" app:app];
    NSLog(@"MOTE BROWSER APPROVED OBSERVED");
    [self assertExactLine:@"Pending" timeout:20 app:app];
    [self capture:@"browser-pending-expiry" app:app];
    [self assertExactLine:@"Expired" timeout:150 app:app];
    [self capture:@"browser-expired" app:app];
    for (VNRecognizedTextObservation *item in [self recognize:app]) {
        NSString *text = [item topCandidates:1].firstObject.string;
        XCTAssertFalse(statusTextMatches(text, @"Approve") || statusTextMatches(text, @"Deny"));
    }
    [self assertText:@"Approved request completed." app:app];
    NSLog(@"MOTE BROWSER EXPIRED OBSERVED");
}
@end
