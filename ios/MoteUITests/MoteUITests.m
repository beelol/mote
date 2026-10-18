#import <XCTest/XCTest.h>
#import <TargetConditionals.h>
#import "../Mote/mote_ios.h"
#import <objc/runtime.h>
@import Vision;

static NSUInteger motionNotifications;
static void observedMotionChange(void) {
    motionNotifications++;
}

static NSArray<NSString *> *galleryPages(void) {
    return @[@"Layout",@"Toggle",@"Slider",@"Select",@"List",@"Form",@"Dashboard",@"Fixture"];
}

static CGPoint galleryTapPoint(CGRect frame, NSUInteger position) {
    if (!isfinite(frame.origin.x) || !isfinite(frame.origin.y) ||
        !isfinite(frame.size.width) || !isfinite(frame.size.height) ||
        frame.size.width < 48 || frame.size.height < 48 || position > 4) {
        return CGPointMake(NAN, NAN);
    }
    CGPoint point = CGPointMake(CGRectGetMidX(frame), CGRectGetMidY(frame));
    if (position == 1) point.x = CGRectGetMinX(frame) + 2;
    if (position == 2) point.x = CGRectGetMaxX(frame) - 2;
    if (position == 3) point.y = CGRectGetMinY(frame) + 2;
    if (position == 4) point.y = CGRectGetMaxY(frame) - 2;
    return point;
}

static NSArray<NSDictionary *> *galleryControls(NSString *page) {
    if ([page isEqualToString:@"List"]) {
        NSMutableArray *controls = [NSMutableArray array];
        NSArray *first = @[@"draft",@"review",@"ready"];
        for (NSUInteger index = 0; index < 20; index++) {
            NSString *label = index < 3 ? [@"Open " stringByAppendingString:first[index]]
                : [NSString stringWithFormat:@"Open item %lu", (unsigned long)index + 1];
            NSString *event = index < 3 ? [@"gallery." stringByAppendingString:first[index]]
                : [NSString stringWithFormat:@"gallery.item.%lu", (unsigned long)index + 1];
            [controls addObject:@{@"kind":@"action",@"label":label,@"event":event}];
        }
        return controls;
    }
    NSArray *priority = @[@"Low",@"Normal",@"High"];
    NSArray *models = @[@"qwen3:4b",@"qwen3:0.6b",@"qwen3-coder:30b-64k"];
    NSArray *volume = @[@6,@0,@10,@6,@6];
    NSDictionary *pages = @{
        @"Layout":@[@{@"kind":@"action",@"label":@"Preview",@"event":@"gallery.layout"}],
        @"Toggle":@[@{@"kind":@"toggle",@"label":@"Enable preview"}],
        @"Slider":@[@{@"kind":@"slider",@"label":@"Volume",@"values":volume,@"max":@10}],
        @"Select":@[@{@"kind":@"select",@"label":@"Priority",@"options":priority}],
        @"Form":@[
            @{@"kind":@"toggle",@"label":@"Enable preview"},
            @{@"kind":@"select",@"label":@"Priority",@"options":priority},
            @{@"kind":@"slider",@"label":@"Volume",@"values":volume,@"max":@10},
            @{@"kind":@"action",@"label":@"Submit preview",@"event":@"gallery.submit"}],
        @"Dashboard":@[
            @{@"kind":@"toggle",@"label":@"Local only"},
            @{@"kind":@"select",@"label":@"Talk model",@"options":models},
            @{@"kind":@"select",@"label":@"Router model",@"options":models},
            @{@"kind":@"select",@"label":@"Think model",@"options":models},
            @{@"kind":@"action",@"label":@"Refresh",@"event":@"refresh"}],
        @"Fixture":@[
            @{@"kind":@"toggle",@"label":@"Local only"},
            @{@"kind":@"slider",@"label":@"Volume",@"values":@[@50,@0,@100,@50,@50],@"max":@100},
            @{@"kind":@"select",@"label":@"Mode",@"options":@[@"quiet",@"normal"]},
            @{@"kind":@"action",@"label":@"Refresh",@"event":@"refresh"}]
    };
    return pages[page] ?: @[];
}

static NSArray<NSArray<NSString *> *> *galleryPhysicalBatches(NSString *page) {
    NSArray *labels = [galleryControls(page) valueForKey:@"label"];
    if (!labels.count) return @[];
    if ([page isEqual:@"Dashboard"]) {
        return @[@[@"Local only",@"Refresh"],@[@"Talk model"],@[@"Router model"],@[@"Think model"]];
    }
    if (![page isEqual:@"List"]) return @[labels];
    NSMutableArray *batches = [NSMutableArray array];
    for (NSUInteger start = 0; start < labels.count; start += 4) {
        [batches addObject:[labels subarrayWithRange:NSMakeRange(start, MIN((NSUInteger)4, labels.count - start))]];
    }
    return batches;
}

static NSArray<NSString *> *galleryVoiceOverControlOrder(NSString *page) {
    if ([page isEqualToString:@"Dashboard"]) {
        return @[@"Refresh", @"Local only", @"Talk model", @"Router model", @"Think model"];
    }
    return [galleryControls(page) valueForKey:@"label"];
}

static NSString *compactText(NSString *text) {
    return [[text componentsSeparatedByCharactersInSet:NSCharacterSet.whitespaceAndNewlineCharacterSet] componentsJoinedByString:@""];
}

static NSArray<NSString *> *galleryStableConversationLabels(NSArray<NSString *> *labels) {
    NSMutableOrderedSet<NSString *> *stable = [NSMutableOrderedSet orderedSet];
    for (NSString *label in labels) {
        if ([label hasPrefix:@"User: "]) [stable addObject:label];
    }
    return stable.array;
}

static BOOL galleryLabelsContainOrderedSubsequence(NSArray<NSString *> *labels,
                                                   NSArray<NSString *> *expected) {
    NSUInteger next = 0;
    for (NSString *label in labels) {
        if (next < expected.count && [label isEqualToString:expected[next]]) next++;
    }
    return next == expected.count;
}

static BOOL galleryInventoryMatches(NSDictionary *observed, NSDictionary *expected) {
    return [observed isEqualToDictionary:expected];
}

static BOOL galleryBackgroundEvidenceSufficient(BOOL springboardVisible, BOOL homeIconHittable,
                                                XCUIApplicationState moteState) {
    BOOL moteBackgrounded = moteState == XCUIApplicationStateRunningBackground ||
                            moteState == XCUIApplicationStateRunningBackgroundSuspended;
    return springboardVisible && homeIconHittable && moteBackgrounded;
}

static NSString *galleryApplicationStateName(XCUIApplicationState state) {
    switch (state) {
        case XCUIApplicationStateUnknown: return @"unknown";
        case XCUIApplicationStateNotRunning: return @"not-running";
        case XCUIApplicationStateRunningBackgroundSuspended: return @"background-suspended";
        case XCUIApplicationStateRunningBackground: return @"background-running";
        case XCUIApplicationStateRunningForeground: return @"foreground";
    }
    return @"unrecognized";
}

static double galleryColorFraction(CGImageRef image, CGRect region,
                                   uint8_t red, uint8_t green, uint8_t blue, uint8_t tolerance) {
    if (!image || !isfinite(region.origin.x) || !isfinite(region.origin.y) ||
        !isfinite(region.size.width) || !isfinite(region.size.height) ||
        region.size.width <= 0 || region.size.height <= 0) return NAN;
    region = CGRectIntegral(region);
    CGRect bounds = CGRectMake(0, 0, CGImageGetWidth(image), CGImageGetHeight(image));
    if (!CGRectContainsRect(bounds, region)) return NAN;
    CGImageRef crop = CGImageCreateWithImageInRect(image, region);
    if (!crop) return NAN;
    size_t width = CGImageGetWidth(crop), height = CGImageGetHeight(crop);
    if (!width || !height || width > SIZE_MAX / 4 || height > SIZE_MAX / (width * 4)) {
        CGImageRelease(crop);
        return NAN;
    }
    @autoreleasepool {
        NSMutableData *pixels = [NSMutableData dataWithLength:width * height * 4];
        CGColorSpaceRef colors = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
        CGContextRef context = colors ? CGBitmapContextCreate(pixels.mutableBytes, width, height, 8,
            width * 4, colors, kCGImageAlphaPremultipliedLast | kCGBitmapByteOrder32Big) : NULL;
        if (colors) CGColorSpaceRelease(colors);
        if (!context) {
            CGImageRelease(crop);
            return NAN;
        }
        CGContextDrawImage(context, CGRectMake(0, 0, width, height), crop);
        CGContextRelease(context);
        CGImageRelease(crop);
        const uint8_t *bytes = pixels.bytes;
        NSUInteger matching = 0;
        for (size_t index = 0; index < width * height; index++) {
            const uint8_t *pixel = bytes + index * 4;
            if (pixel[3] >= 250 && abs((int)pixel[0] - red) <= tolerance &&
                abs((int)pixel[1] - green) <= tolerance && abs((int)pixel[2] - blue) <= tolerance) matching++;
        }
        return (double)matching / (width * height);
    }
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

static CGRect textTargetBox(NSArray<NSDictionary *> *fragments, NSString *target, CGFloat height) {
    NSString *compactTarget = compactText(target);
    for (NSInteger exact = 1; exact >= 0; exact--) {
        for (NSDictionary *fragment in fragments) {
            NSString *text = compactText(fragment[@"text"]);
            if (exact ? [text isEqualToString:compactTarget] : [text containsString:compactTarget]) {
                return [fragment[@"box"] CGRectValue];
            }
        }
    }
    for (NSDictionary *anchor in fragments) {
        CGFloat center = CGRectGetMidY([anchor[@"box"] CGRectValue]);
        NSMutableArray<NSDictionary *> *row = [NSMutableArray array];
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
        for (NSUInteger start = 0; start < row.count; start++) {
            NSMutableString *joined = [NSMutableString string];
            CGRect box = CGRectNull;
            for (NSUInteger end = start; end < row.count; end++) {
                [joined appendString:compactText(row[end][@"text"])];
                box = CGRectIsNull(box) ? [row[end][@"box"] CGRectValue]
                                             : CGRectUnion(box, [row[end][@"box"] CGRectValue]);
                if ([joined containsString:compactTarget]) return box;
            }
        }
    }
    return CGRectNull;
}

@interface MoteUITests : XCTestCase
@property(nonatomic) NSUInteger recognitionMisses;
@end

@implementation MoteUITests
- (void)setUp {
    [super setUp];
    self.continueAfterFailure = NO;
}

- (void)testNativeMotionPreferenceAndObserverLifetime {
    XCTAssertEqual(mote_ios_reduce_motion_enabled(), (uint8_t)UIAccessibilityIsReduceMotionEnabled());
    XCTAssertEqual(mote_ios_motion_observe(NULL), NULL);
    mote_ios_motion_remove(NULL);
    motionNotifications = 0;
    for (NSUInteger lifetime = 0; lifetime < 3; lifetime++) {
        void *observer = mote_ios_motion_observe(observedMotionChange);
        XCTAssertNotEqual(observer, NULL);
        for (NSUInteger notification = 0; notification < 3; notification++) {
            [NSNotificationCenter.defaultCenter postNotificationName:UIAccessibilityReduceMotionStatusDidChangeNotification object:nil];
            XCTAssertEqual(motionNotifications, lifetime * 3 + notification + 1);
        }
        mote_ios_motion_remove(observer);
        [NSNotificationCenter.defaultCenter postNotificationName:UIAccessibilityReduceMotionStatusDidChangeNotification object:nil];
        XCTAssertEqual(motionNotifications, (lifetime + 1) * 3);
    }
}

- (void)testSystemThemePixelSamplerRejectsWrongColorsAndInvalidRegions {
    UIGraphicsImageRendererFormat *format = [UIGraphicsImageRendererFormat defaultFormat];
    format.scale = 1;
    format.opaque = NO;
    format.preferredRange = UIGraphicsImageRendererFormatRangeStandard;
    UIGraphicsImageRenderer *renderer = [[UIGraphicsImageRenderer alloc] initWithSize:CGSizeMake(8, 4) format:format];
    UIImage *image = [renderer imageWithActions:^(UIGraphicsImageRendererContext *context) {
        [[UIColor colorWithRed:244.0/255 green:241.0/255 blue:234.0/255 alpha:1] setFill];
        UIRectFill(CGRectMake(0, 0, 4, 4));
        [[UIColor colorWithRed:26.0/255 green:26.0/255 blue:26.0/255 alpha:1] setFill];
        UIRectFill(CGRectMake(4, 0, 4, 4));
    }];
    CGRect paper = CGRectMake(0, 0, 4, 4);
    CGRect ink = CGRectMake(4, 0, 4, 4);
    XCTAssertEqualWithAccuracy(galleryColorFraction(image.CGImage, paper, 244, 241, 234, 0), 1, 0.0001);
    XCTAssertEqualWithAccuracy(galleryColorFraction(image.CGImage, ink, 26, 26, 26, 0), 1, 0.0001);
    XCTAssertEqualWithAccuracy(galleryColorFraction(image.CGImage, paper, 11, 11, 12, 4), 0, 0.0001);
    UIImage *dark = [renderer imageWithActions:^(UIGraphicsImageRendererContext *context) {
        [[UIColor colorWithRed:11.0/255 green:11.0/255 blue:12.0/255 alpha:1] setFill];
        UIRectFill(paper);
        [[UIColor colorWithRed:232.0/255 green:227.0/255 blue:218.0/255 alpha:1] setFill];
        UIRectFill(ink);
    }];
    XCTAssertEqualWithAccuracy(galleryColorFraction(dark.CGImage, paper, 11, 11, 12, 0), 1, 0.0001);
    XCTAssertEqualWithAccuracy(galleryColorFraction(dark.CGImage, ink, 232, 227, 218, 0), 1, 0.0001);
    XCTAssertEqualWithAccuracy(galleryColorFraction(dark.CGImage, paper, 244, 241, 234, 4), 0, 0.0001);
    XCTAssertEqualWithAccuracy(galleryColorFraction(dark.CGImage, ink, 26, 26, 26, 8), 0, 0.0001);
    XCTAssertEqualWithAccuracy(galleryColorFraction(image.CGImage, ink, 232, 227, 218, 8), 0, 0.0001);
    XCTAssertEqualWithAccuracy(galleryColorFraction(image.CGImage, paper, 240, 237, 230, 4), 1, 0.0001);
    XCTAssertEqualWithAccuracy(galleryColorFraction(image.CGImage, paper, 240, 237, 230, 3), 0, 0.0001);
    XCTAssertEqualWithAccuracy(galleryColorFraction(image.CGImage, CGRectMake(0, 0, 8, 4), 244, 241, 234, 0), 0.5, 0.0001);
    for (NSValue *invalid in @[[NSValue valueWithCGRect:CGRectZero],
        [NSValue valueWithCGRect:CGRectMake(-1, 0, 4, 4)],
        [NSValue valueWithCGRect:CGRectMake(5, 0, 4, 4)],
        [NSValue valueWithCGRect:CGRectMake(NAN, 0, 4, 4)]]) {
        XCTAssertTrue(isnan(galleryColorFraction(image.CGImage, invalid.CGRectValue, 0, 0, 0, 0)));
    }
    XCTAssertTrue(isnan(galleryColorFraction(NULL, paper, 0, 0, 0, 0)));
    UIImage *transparent = [renderer imageWithActions:^(UIGraphicsImageRendererContext *context) {}];
    XCTAssertEqualWithAccuracy(galleryColorFraction(transparent.CGImage, paper, 0, 0, 0, 0), 0, 0.0001);
}

- (void)testGalleryTapPointsCoverCenterAndFourInsetEdges {
    CGRect frame = CGRectMake(101.5, 203.25, 96, 48);
    NSArray<NSValue *> *expected = @[
        [NSValue valueWithCGPoint:CGPointMake(149.5, 227.25)],
        [NSValue valueWithCGPoint:CGPointMake(103.5, 227.25)],
        [NSValue valueWithCGPoint:CGPointMake(195.5, 227.25)],
        [NSValue valueWithCGPoint:CGPointMake(149.5, 205.25)],
        [NSValue valueWithCGPoint:CGPointMake(149.5, 249.25)]
    ];
    for (NSUInteger position = 0; position < expected.count; position++) {
        CGPoint point = galleryTapPoint(frame, position);
        XCTAssertTrue(CGPointEqualToPoint(point, expected[position].CGPointValue));
        XCTAssertTrue(CGRectContainsPoint(frame, point));
    }
    for (NSValue *invalid in @[[NSValue valueWithCGRect:CGRectZero],
                               [NSValue valueWithCGRect:CGRectMake(0, 0, 47, 48)],
                               [NSValue valueWithCGRect:CGRectMake(NAN, 0, 48, 48)]]) {
        CGPoint point = galleryTapPoint(invalid.CGRectValue, 0);
        XCTAssertFalse(isfinite(point.x) && isfinite(point.y));
    }
    CGPoint unknown = galleryTapPoint(frame, 5);
    XCTAssertFalse(isfinite(unknown.x) && isfinite(unknown.y));
}

- (void)testGalleryControlInventoryIncludesEveryPageAndListAction {
    NSDictionary *counts = @{@"Layout":@1,@"Toggle":@1,@"Slider":@1,@"Select":@1,
                             @"List":@20,@"Form":@4,@"Dashboard":@5,@"Fixture":@4};
    XCTAssertEqualObjects([NSSet setWithArray:galleryPages()], [NSSet setWithArray:counts.allKeys]);
    XCTAssertEqual(galleryPages().count, (NSUInteger)8);
    NSArray *kinds = @[@"action",@"toggle",@"slider",@"select"];
    for (NSString *page in counts) {
        NSArray *controls = galleryControls(page);
        XCTAssertEqual(controls.count, [counts[page] unsignedIntegerValue], @"%@", page);
        NSMutableSet *labels = [NSMutableSet set];
        for (NSDictionary *control in controls) {
            XCTAssertFalse([labels containsObject:control[@"label"]]);
            [labels addObject:control[@"label"]];
            XCTAssertTrue([kinds containsObject:control[@"kind"]]);
            if ([control[@"kind"] isEqual:@"select"]) {
                NSArray *options = control[@"options"];
                XCTAssertGreaterThanOrEqual(options.count, (NSUInteger)2);
                XCTAssertEqual([NSSet setWithArray:options].count, options.count);
            }
            if ([control[@"kind"] isEqual:@"slider"]) {
                XCTAssertEqual([control[@"values"] count], (NSUInteger)5);
            }
        }
    }
    NSArray *list = galleryControls(@"List");
    NSArray *first = @[@"Open draft",@"Open review",@"Open ready"];
    for (NSUInteger index = 0; index < 20; index++) {
        NSString *label = index < 3 ? first[index] : [NSString stringWithFormat:@"Open item %lu", (unsigned long)index + 1];
        NSArray *matches = [list filteredArrayUsingPredicate:[NSPredicate predicateWithFormat:@"label == %@", label]];
        XCTAssertEqual(matches.count, (NSUInteger)1);
    }
    XCTAssertEqual(galleryControls(@"Unknown").count, (NSUInteger)0);
}

- (void)testGalleryObservedInventoryRejectsMissingUnexpectedAndWrongRole {
    NSDictionary *expected = @{@"Open item 20":@(XCUIElementTypeButton), @"Volume":@(XCUIElementTypeOther)};
    XCTAssertTrue(galleryInventoryMatches(expected, expected));
    XCTAssertFalse(galleryInventoryMatches(@{@"Volume":@(XCUIElementTypeOther)}, expected));
    XCTAssertFalse(galleryInventoryMatches(@{@"Open item 20":@(XCUIElementTypeButton), @"Volume":@(XCUIElementTypeOther), @"Open item 21":@(XCUIElementTypeButton)}, expected));
    XCTAssertFalse(galleryInventoryMatches(@{@"Open item 20":@(XCUIElementTypeOther), @"Volume":@(XCUIElementTypeOther)}, expected));
}

- (void)testGalleryPhysicalCasesPartitionEveryControlExactlyOnce {
    NSDictionary *counts = @{@"Layout":@1,@"Toggle":@1,@"Slider":@1,@"Select":@1,
                             @"List":@5,@"Form":@1,@"Dashboard":@4,@"Fixture":@1};
    NSUInteger cases = 0;
    for (NSString *page in galleryPages()) {
        NSArray *batches = galleryPhysicalBatches(page);
        XCTAssertEqual(batches.count, [counts[page] unsignedIntegerValue], @"%@", page);
        NSMutableArray *labels = [NSMutableArray array];
        for (NSArray *batch in batches) {
            XCTAssertGreaterThan(batch.count, (NSUInteger)0);
            if ([page isEqual:@"List"]) XCTAssertLessThanOrEqual(batch.count, (NSUInteger)4);
            NSUInteger selects = 0;
            for (NSString *label in batch) {
                NSArray *matches = [galleryControls(page) filteredArrayUsingPredicate:[NSPredicate predicateWithFormat:@"label == %@",label]];
                XCTAssertEqual(matches.count, (NSUInteger)1);
                if ([matches.firstObject[@"kind"] isEqual:@"select"]) selects++;
            }
            XCTAssertLessThanOrEqual(selects, (NSUInteger)1);
            [labels addObjectsFromArray:batch];
        }
        XCTAssertEqual([NSSet setWithArray:labels].count, labels.count);
        XCTAssertEqualObjects([NSSet setWithArray:labels], [NSSet setWithArray:[galleryControls(page) valueForKey:@"label"]]);
        cases += batches.count;
    }
    XCTAssertEqual(cases * 2 + 2, (NSUInteger)32);
    XCTAssertEqual(galleryPhysicalBatches(@"Unknown").count, (NSUInteger)0);
    NSArray *stems = @[@"Layout",@"Toggle",@"Slider",@"Select",@"ListItems01Through04",@"ListItems05Through08",
        @"ListItems09Through12",@"ListItems13Through16",@"ListItems17Through20",@"Form",@"DashboardBase",
        @"DashboardTalkModel",@"DashboardRouterModel",@"DashboardThinkModel",@"Fixture",@"PreviewNote"];
    NSMutableSet *expectedSelectors = [NSMutableSet set];
    for (NSString *orientation in @[@"Portrait",@"Landscape"]) {
        for (NSString *stem in stems) {
            [expectedSelectors addObject:[NSString stringWithFormat:@"testPhysicalGallery%@%@",stem,orientation]];
        }
    }
    unsigned int methodCount = 0;
    Method *methods = class_copyMethodList(self.class, &methodCount);
    NSMutableSet *observedSelectors = [NSMutableSet set];
    for (unsigned int index = 0; index < methodCount; index++) {
        NSString *name = NSStringFromSelector(method_getName(methods[index]));
        if ([name hasPrefix:@"testPhysicalGallery"]) [observedSelectors addObject:name];
    }
    free(methods);
    XCTAssertEqualObjects(observedSelectors, expectedSelectors);
}

- (void)testGalleryBackgroundEvidenceRequiresHomeAndHiddenMote {
    XCTAssertTrue(galleryBackgroundEvidenceSufficient(
        YES, YES, XCUIApplicationStateRunningBackground));
    XCTAssertTrue(galleryBackgroundEvidenceSufficient(
        YES, YES, XCUIApplicationStateRunningBackgroundSuspended));
    for (NSNumber *state in @[@(XCUIApplicationStateUnknown), @(XCUIApplicationStateNotRunning),
                               @(XCUIApplicationStateRunningForeground)]) {
        XCTAssertFalse(galleryBackgroundEvidenceSufficient(YES, YES, state.integerValue));
    }
    XCTAssertFalse(galleryBackgroundEvidenceSufficient(
        NO, YES, XCUIApplicationStateRunningBackground));
    XCTAssertFalse(galleryBackgroundEvidenceSufficient(
        YES, NO, XCUIApplicationStateRunningBackground));
    XCTAssertEqualObjects(galleryApplicationStateName(XCUIApplicationStateRunningBackground), @"background-running");
    XCTAssertEqualObjects(galleryApplicationStateName(XCUIApplicationStateRunningBackgroundSuspended), @"background-suspended");
    XCTAssertEqualObjects(galleryApplicationStateName(XCUIApplicationStateUnknown), @"unknown");
    XCTAssertEqualObjects(galleryApplicationStateName(XCUIApplicationStateNotRunning), @"not-running");
    XCTAssertEqualObjects(galleryApplicationStateName(XCUIApplicationStateRunningForeground), @"foreground");
    XCTAssertEqualObjects(galleryApplicationStateName((XCUIApplicationState)999), @"unrecognized");
    XCTAssertEqualObjects(galleryStableConversationLabels(
        @[@"3:43 PM", @"User: one", @"Cursor: 9", @"User: one", @"User: two"]),
        (@[@"User: one", @"User: two"]));
    XCTAssertTrue(galleryLabelsContainOrderedSubsequence(
        @[@"Layout", @"ignored", @"Toggle", @"Slider"], @[@"Layout", @"Toggle", @"Slider"]));
    XCTAssertFalse(galleryLabelsContainOrderedSubsequence(
        @[@"Toggle", @"Layout", @"Slider"], @[@"Layout", @"Toggle", @"Slider"]));
}
- (void)tearDown {
    self.recognitionMisses = 0;
    XCUIDevice.sharedDevice.orientation = UIDeviceOrientationPortrait;
    [super tearDown];
}

- (XCUIElement *)nativeElement:(NSString *)label app:(XCUIApplication *)app {
    return [[[app descendantsMatchingType:XCUIElementTypeAny]
        matchingPredicate:[NSPredicate predicateWithFormat:@"label == %@", label]] firstMatch];
}

- (void)assertValue:(NSString *)value element:(XCUIElement *)element {
    NSPredicate *predicate = [NSPredicate predicateWithFormat:@"value == %@", value];
    XCTNSPredicateExpectation *expectation =
        [[XCTNSPredicateExpectation alloc] initWithPredicate:predicate object:element];
    XCTAssertEqual([XCTWaiter waitForExpectations:@[expectation] timeout:5],
                   XCTWaiterResultCompleted, @"Expected value %@, observed %@", value, element.value);
}

- (void)testMatrixControlsAppearAsIndividualNativeAccessibilityElements {
    XCUIApplication *app = [XCUIApplication new];
    app.launchArguments = @[@"--mote-offline", @"--mote-page", @"toggle"];
    [app launch];

    XCUIElement *togglePage = app.buttons[@"Toggle"];
    XCUIElement *toggle = app.buttons[@"Enable preview"];
    XCTAssertTrue([togglePage waitForExistenceWithTimeout:10]);
    XCTAssertTrue([toggle waitForExistenceWithTimeout:10]);
    XCTAssertEqualObjects(toggle.value, @"On");
    XCTAssertGreaterThanOrEqual(toggle.frame.size.height, 48.0);
    [toggle tap];
    XCTAssertEqualObjects(toggle.value, @"Off");

    [app.buttons[@"Slider"] tap];
    XCUIElement *slider = app.otherElements[@"Volume"];
    XCTAssertTrue([slider waitForExistenceWithTimeout:10]);
    XCTAssertEqualObjects(slider.value, @"4");
    [[slider coordinateWithNormalizedOffset:CGVectorMake(0.8, 0.5)] tap];
    XCTAssertEqualObjects(slider.value, @"8");

    [app.buttons[@"Select"] tap];
    XCUIElement *select = app.buttons[@"Priority"];
    XCTAssertTrue([select waitForExistenceWithTimeout:10]);
    XCTAssertEqualObjects(select.value, @"Normal");
    [select tap];
    XCUIElement *high = app.buttons[@"High"];
    XCTAssertTrue([high waitForExistenceWithTimeout:10]);
    [high tap];
    XCTAssertEqualObjects(select.value, @"High");
    XCTAssertFalse(toggle.exists);
}

- (void)testNativeTextBridgeSupportsEdgeFocusHardwareEditingAndEditMenu {
    XCUIApplication *app = [XCUIApplication new];
    app.launchArguments = @[@"--mote-offline", @"--mote-test-pasteboard", @"pasted"];
    [app launch];
    XCTAssertTrue([app waitForState:XCUIApplicationStateRunningForeground timeout:15]);

    XCUIElement *field = [self nativeElement:@"Preview note" app:app];
    XCTAssertTrue([field waitForExistenceWithTimeout:10]);
    XCTAssertGreaterThanOrEqual(field.frame.size.height, 48.0);
    XCUICoordinate *rightEdge = [field coordinateWithNormalizedOffset:CGVectorMake(0.98, 0.5)];
    [rightEdge tap];
    XCTAssertTrue([app.keyboards.firstMatch waitForExistenceWithTimeout:5]);

    XCUIElement *editable = app.textFields[@"mote.native-text-input"];
    XCTAssertTrue([editable waitForExistenceWithTimeout:5]);
    [editable typeText:@"abcd"];
    [editable typeKey:XCUIKeyboardKeyLeftArrow modifierFlags:0];
    [editable typeText:@"X"];
    [self assertValue:@"abcXd" element:editable];

    [editable doubleTap];
    XCUIElement *paste = [self nativeElement:@"Paste" app:app];
    XCTAssertTrue([paste waitForExistenceWithTimeout:5]);
    [paste tap];
    [self assertValue:@"pasted" element:editable];

    [editable doubleTap];
    XCUIElement *copy = [self nativeElement:@"Copy" app:app];
    XCTAssertTrue([copy waitForExistenceWithTimeout:5]);
    [copy tap];

    [editable doubleTap];
    XCUIElement *cut = [self nativeElement:@"Cut" app:app];
    XCTAssertTrue([cut waitForExistenceWithTimeout:5]);
    [cut tap];
    NSPredicate *cleared = [NSPredicate predicateWithFormat:@"value == '' OR value == nil"];
    XCTNSPredicateExpectation *clearExpectation =
        [[XCTNSPredicateExpectation alloc] initWithPredicate:cleared object:editable];
    XCTAssertEqual([XCTWaiter waitForExpectations:@[clearExpectation] timeout:5],
                   XCTWaiterResultCompleted);

    [editable pressForDuration:1.0];
    paste = [self nativeElement:@"Paste" app:app];
    XCTAssertTrue([paste waitForExistenceWithTimeout:5]);
    [paste tap];
    [self assertValue:@"pasted" element:editable];
    [self capture:@"native-text-edit-menu" app:app];
}

- (void)testNativeTextBridgePreservesMarkedTextAndAcceptsDictationFallback {
    XCUIApplication *app = [XCUIApplication new];
    app.launchArguments = @[@"--mote-offline", @"--mote-test-native-input-sequence"];
    [app launch];
    XCTAssertTrue([app waitForState:XCUIApplicationStateRunningForeground timeout:15]);

    XCUIElement *field = [self nativeElement:@"Preview note" app:app];
    XCTAssertTrue([field waitForExistenceWithTimeout:10]);
    [field tap];

    XCUIElement *editable = app.textFields[@"mote.native-text-input"];
    XCTAssertTrue([editable waitForExistenceWithTimeout:5]);
    [self assertValue:@"かな Mote dictation works" element:editable];
    [self capture:@"native-text-composition-dictation" app:app];
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

- (CGRect)findTextBox:(NSString *)text app:(XCUIApplication *)app {
    __block CGRect found = CGRectNull;
    NSPredicate *predicate = [NSPredicate predicateWithBlock:^BOOL(id object, NSDictionary *bindings) {
        NSMutableArray *fragments = [NSMutableArray array];
        for (VNRecognizedTextObservation *item in [self recognize:app]) {
            [fragments addObject:@{@"text": [item topCandidates:1].firstObject.string,
                                   @"box": [NSValue valueWithCGRect:item.boundingBox]}];
        }
        found = textTargetBox(fragments, text, app.windows.firstMatch.frame.size.height);
        return !CGRectIsNull(found);
    }];
    XCTNSPredicateExpectation *expectation = [[XCTNSPredicateExpectation alloc] initWithPredicate:predicate object:app];
    XCTAssertEqual([XCTWaiter waitForExpectations:@[expectation] timeout:8], XCTWaiterResultCompleted,
                   @"Cannot locate %@ in %@", text, [self visibleText:app]);
    return found;
}

- (void)tapText:(NSString *)text app:(XCUIApplication *)app {
    CGRect box = [self findTextBox:text app:app];
    CGVector normalized = CGVectorMake(CGRectGetMidX(box), 1.0 - CGRectGetMidY(box));
    NSLog(@"MOTE TAP %@ point=(%.2f, %.2f)", text, normalized.dx * app.windows.firstMatch.frame.size.width, normalized.dy * app.windows.firstMatch.frame.size.height);
    [[app.windows.firstMatch coordinateWithNormalizedOffset:normalized] tap];
}

- (CGRect)moveSlider:(XCUIApplication *)app {
    CGRect item = [self findTextBox:@"Volume" app:app];
    CGRect window = app.windows.firstMatch.frame;
    CGPoint point = CGPointMake(CGRectGetMinX(item) * window.size.width + 4.0,
                                (1.0 - CGRectGetMinY(item)) * window.size.height + 16.0);
    NSLog(@"MOTE SLIDER point=(%.2f, %.2f)", point.x, point.y);
    [[[app.windows.firstMatch coordinateWithNormalizedOffset:CGVectorMake(0, 0)] coordinateWithOffset:CGVectorMake(point.x, point.y)] tap];
    return item;
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

- (XCUIElement *)galleryElement:(NSString *)label kind:(NSString *)kind app:(XCUIApplication *)app {
    XCUIElement *element = [kind isEqualToString:@"slider"] ? app.otherElements[label] : app.buttons[label];
    BOOL appeared = [element waitForExistenceWithTimeout:1];
    if (!appeared) NSLog(@"GALLERY SEEK label=%@", label);
    for (NSUInteger attempt = 0; attempt < 24; attempt++) {
        BOOL exists = element.exists;
        CGRect frame = exists ? element.frame : CGRectZero;
        CGPoint point = galleryTapPoint(frame, 0);
        if (exists && isfinite(point.x) && isfinite(point.y) &&
            CGRectContainsRect(app.windows.firstMatch.frame, frame)) {
            XCTAssertTrue(element.enabled, @"%@", label);
            return element;
        }
        BOOL reverse = attempt >= 12 || (exists && CGRectGetMidY(frame) < CGRectGetMidY(app.windows.firstMatch.frame));
        XCUICoordinate *start = [app.windows.firstMatch coordinateWithNormalizedOffset:CGVectorMake(0.85, reverse ? 0.4 : 0.75)];
        XCUICoordinate *end = [app.windows.firstMatch coordinateWithNormalizedOffset:CGVectorMake(0.85, reverse ? 0.75 : 0.4)];
        [start pressForDuration:0.05 thenDragToCoordinate:end];
    }
    [self capture:[@"missing-gallery-control-" stringByAppendingString:label] app:app];
    XCTFail(@"No fully visible native hit target for %@", label);
    return element;
}

- (void)tapGalleryElement:(XCUIElement *)element position:(NSUInteger)position app:(XCUIApplication *)app {
    CGRect frame = element.frame;
    CGPoint point = galleryTapPoint(frame, position);
    XCTAssertTrue(isfinite(point.x) && isfinite(point.y), @"Invalid native hit target %@ %@", element.label, NSStringFromCGRect(frame));
    CGRect window = app.windows.firstMatch.frame;
    XCTAssertTrue(CGRectContainsRect(window, frame));
    NSArray *positions = @[@"center",@"left",@"right",@"top",@"bottom"];
    NSLog(@"GALLERY TAP label=%@ position=%@ frame=%@ point=%@", element.label, positions[position], NSStringFromCGRect(frame), NSStringFromCGPoint(point));
    [[[app.windows.firstMatch coordinateWithNormalizedOffset:CGVectorMake(0, 0)]
        coordinateWithOffset:CGVectorMake(point.x - window.origin.x, point.y - window.origin.y)] tap];
}

- (void)selectGalleryPage:(NSString *)page position:(NSUInteger)position app:(XCUIApplication *)app {
    NSString *other = [page isEqualToString:@"Layout"] ? @"Toggle" : @"Layout";
    [app.buttons[other] tap];
    [self assertText:[@"Selected " stringByAppendingString:other] app:app];
    [self tapGalleryElement:[self galleryElement:page kind:@"action" app:app] position:position app:app];
    [self assertText:[@"Selected " stringByAppendingString:page] app:app];
}

- (void)chooseGalleryOption:(NSString *)option label:(NSString *)label position:(NSUInteger)position app:(XCUIApplication *)app {
    [self tapGalleryElement:[self galleryElement:label kind:@"select" app:app] position:position app:app];
    XCUIElement *choice = [self galleryElement:option kind:@"action" app:app];
    [self tapGalleryElement:choice position:position app:app];
    [self assertValue:option element:app.buttons[label]];
}

- (void)exerciseGalleryControl:(NSDictionary *)control page:(NSString *)page app:(XCUIApplication *)app {
    NSString *label = control[@"label"];
    NSString *kind = control[@"kind"];
    if ([kind isEqualToString:@"select"]) {
        NSArray<NSString *> *options = control[@"options"];
        for (NSUInteger position = 0; position < 5; position++) {
            for (NSString *option in options) {
                XCUIElement *select = [self galleryElement:label kind:kind app:app];
                XCTAssertTrue([options containsObject:select.value], @"Unexpected select value %@", select.value);
                if ([option isEqual:select.value]) {
                    NSString *other = [option isEqualToString:options[0]] ? options[1] : options[0];
                    [self chooseGalleryOption:other label:label position:0 app:app];
                }
                [self chooseGalleryOption:option label:label position:position app:app];
            }
        }
        return;
    }
    for (NSUInteger position = 0; position < 5; position++) {
        if ([kind isEqualToString:@"action"]) {
            [app.buttons[page] tap];
            [self assertText:[@"Selected " stringByAppendingString:page] app:app];
        }
        XCUIElement *element = [self galleryElement:label kind:kind app:app];
        if ([kind isEqualToString:@"toggle"]) {
            NSArray *values = @[@"On",@"Off"];
            XCTAssertTrue([values containsObject:element.value], @"Unexpected toggle value %@", element.value);
            NSString *target = [element.value isEqual:@"On"] ? @"Off" : @"On";
            [self tapGalleryElement:element position:position app:app];
            [self assertValue:target element:element];
        } else if ([kind isEqualToString:@"slider"]) {
            NSNumber *target = control[@"values"][position];
            NSNumber *seed = target.doubleValue == 0 ? control[@"max"] : @0;
            [self tapGalleryElement:element position:seed.doubleValue == 0 ? 1 : 2 app:app];
            [self assertValue:seed.stringValue element:element];
            [self tapGalleryElement:element position:position app:app];
            [self assertValue:target.stringValue element:element];
        } else {
            [self tapGalleryElement:element position:position app:app];
            [self assertText:[@"Action: " stringByAppendingString:control[@"event"]] app:app];
        }
    }
}

- (void)testGalleryNativeHelpersExerciseEachControlKindAndScrolledAction {
    XCUIApplication *app = [XCUIApplication new];
    app.launchArguments = @[@"--mote-offline"];
    [app launch];
    XCTAssertTrue([app waitForState:XCUIApplicationStateRunningForeground timeout:15]);
    for (NSString *page in @[@"Layout",@"Toggle",@"Slider",@"Select",@"List"]) {
        [self selectGalleryPage:page position:0 app:app];
        NSDictionary *control = [page isEqualToString:@"List"] ? galleryControls(page).lastObject : galleryControls(page).firstObject;
        [self exerciseGalleryControl:control page:page app:app];
    }
    [self capture:@"gallery-native-control-helpers" app:app];
    [app terminate];
}

- (void)assertObservedGalleryInventory:(NSString *)page app:(XCUIApplication *)app {
    NSMutableDictionary *expected = [NSMutableDictionary dictionary];
    for (NSString *navigation in galleryPages()) expected[navigation] = @(XCUIElementTypeButton);
    expected[@"Preview note"] = @(XCUIElementTypeOther);
    for (NSDictionary *control in galleryControls(page)) {
        expected[control[@"label"]] = @([control[@"kind"] isEqual:@"slider"] ? XCUIElementTypeOther : XCUIElementTypeButton);
    }
    NSMutableDictionary *observed = [NSMutableDictionary dictionary];
    for (NSNumber *reverse in @[@NO,@YES]) {
        NSString *previous = nil;
        NSUInteger stable = 0;
        BOOL boundary = NO;
        for (NSUInteger attempt = 0; attempt < 24; attempt++) {
            NSMutableArray *geometry = [NSMutableArray array];
            NSMutableSet *snapshotLabels = [NSMutableSet set];
            NSPredicate *actionable = [NSPredicate predicateWithFormat:@"label != '' AND (elementType == %lu OR elementType == %lu OR elementType == %lu)",
                (unsigned long)XCUIElementTypeButton, (unsigned long)XCUIElementTypeOther, (unsigned long)XCUIElementTypeTextField];
            NSArray *elements = [[app descendantsMatchingType:XCUIElementTypeAny] matchingPredicate:actionable].allElementsBoundByIndex;
            for (XCUIElement *element in elements) {
                NSString *label = element.label;
                NSNumber *role = @(element.elementType);
                XCTAssertFalse([snapshotLabels containsObject:label], @"Duplicate actionable label %@", label);
                [snapshotLabels addObject:label];
                if (observed[label]) XCTAssertEqualObjects(observed[label], role);
                observed[label] = role;
                [geometry addObject:[NSString stringWithFormat:@"%@:%@:%@",label,role,NSStringFromCGRect(element.frame)]];
            }
            NSString *signature = [[geometry sortedArrayUsingSelector:@selector(compare:)] componentsJoinedByString:@"\n"];
            stable = [signature isEqualToString:previous] ? stable + 1 : 0;
            if (stable == 2) { boundary = YES; break; }
            previous = signature;
            XCUICoordinate *start = [app.windows.firstMatch coordinateWithNormalizedOffset:CGVectorMake(0.85, reverse.boolValue ? 0.4 : 0.75)];
            XCUICoordinate *end = [app.windows.firstMatch coordinateWithNormalizedOffset:CGVectorMake(0.85, reverse.boolValue ? 0.75 : 0.4)];
            [start pressForDuration:0.05 thenDragToCoordinate:end];
        }
        XCTAssertTrue(boundary, @"Did not observe a stable scroll boundary for %@", page);
    }
    XCTAttachment *inventory = [XCTAttachment attachmentWithString:[observed description]];
    inventory.name = [@"observed-native-inventory-" stringByAppendingString:page];
    inventory.lifetime = XCTAttachmentLifetimeKeepAlways;
    [self addAttachment:inventory];
    XCTAssertTrue(galleryInventoryMatches(observed, expected), @"%@ observed %@ expected %@",page,observed,expected);
    for (NSDictionary *control in galleryControls(page)) {
        if (![control[@"kind"] isEqual:@"select"]) continue;
        [[self galleryElement:control[@"label"] kind:@"select" app:app] tap];
        NSMutableSet *observedOptions = [NSMutableSet set];
        for (XCUIElement *button in app.buttons.allElementsBoundByIndex) {
            if (!expected[button.label]) [observedOptions addObject:button.label];
        }
        XCTAssertEqualObjects(observedOptions, [NSSet setWithArray:control[@"options"]], @"Unexpected native options for %@",control[@"label"]);
        XCTAttachment *options = [XCTAttachment attachmentWithString:observedOptions.description];
        options.name = [NSString stringWithFormat:@"observed-native-options-%@-%@",page,control[@"label"]];
        options.lifetime = XCTAttachmentLifetimeKeepAlways;
        [self addAttachment:options];
        [app.buttons[[control[@"options"] firstObject]] tap];
    }
}

- (void)exercisePreviewNoteAtEveryPosition:(XCUIApplication *)app {
    for (NSUInteger position = 0; position < 5; position++) {
        [app terminate];
        [app launch];
        XCTAssertFalse(app.keyboards.firstMatch.exists);
        XCUIElement *field = [self nativeElement:@"Preview note" app:app];
        XCTAssertTrue([field waitForExistenceWithTimeout:10]);
        [self tapGalleryElement:field position:position app:app];
        XCTAssertTrue([app.keyboards.firstMatch waitForExistenceWithTimeout:5]);
        XCUIElement *editable = app.textFields[@"mote.native-text-input"];
        XCTAssertTrue([editable waitForExistenceWithTimeout:5]);
        NSString *marker = [NSString stringWithFormat:@"edge%lu",(unsigned long)position];
        [editable typeText:marker];
        [self assertValue:marker element:editable];
        [self capture:[NSString stringWithFormat:@"preview-note-position-%lu",(unsigned long)position] app:app];
    }
    [app terminate];
    [app launch];
}

- (void)testGalleryObservedInventoryCoversEveryPageIncludingAllListRows {
    XCUIApplication *app = [XCUIApplication new];
    app.launchArguments = @[@"--mote-offline"];
    [app launch];
    for (NSString *page in galleryPages()) {
        [self selectGalleryPage:page position:0 app:app];
        [self assertObservedGalleryInventory:page app:app];
    }
    [app terminate];
}

- (void)testGalleryPreviewNoteFreshFocusAndEditingAtEveryPosition {
    XCUIApplication *app = [XCUIApplication new];
    app.launchArguments = @[@"--mote-offline"];
    [self exercisePreviewNoteAtEveryPosition:app];
    [app terminate];
}

- (void)runPhysicalGalleryPage:(NSString *)page orientation:(UIDeviceOrientation)orientation batch:(NSUInteger)batch {
#if TARGET_OS_SIMULATOR
    XCTSkip(@"Requires physical iPad touch evidence");
#endif
    self.executionTimeAllowance = 240;
    NSArray *batches = galleryPhysicalBatches(page);
    XCTAssertLessThan(batch, batches.count);
    NSArray *labels = batches[batch];
    XCUIApplication *app = [XCUIApplication new];
    app.launchArguments = @[@"--mote-offline"];
    XCUIDevice.sharedDevice.orientation = orientation;
    [app launch];
    XCTAssertTrue([app waitForState:XCUIApplicationStateRunningForeground timeout:15]);
    [self selectGalleryPage:page position:0 app:app];
    [self assertObservedGalleryInventory:page app:app];
    for (NSUInteger position = 0; position < 5; position++) {
        [self selectGalleryPage:page position:position app:app];
    }
    NSString *name = [NSString stringWithFormat:@"physical-exhaustive-%ld-%@-%lu",(long)orientation,page,(unsigned long)batch];
    for (NSDictionary *control in galleryControls(page)) {
        if (![labels containsObject:control[@"label"]]) continue;
        [self exerciseGalleryControl:control page:page app:app];
        XCTAttachment *tree = [XCTAttachment attachmentWithString:app.debugDescription];
        tree.name = [NSString stringWithFormat:@"%@-%@-role-tree",name,control[@"label"]];
        tree.lifetime = XCTAttachmentLifetimeKeepAlways;
        [self addAttachment:tree];
    }
    [self capture:name app:app];
    [app terminate];
}

- (void)runPhysicalPreviewNoteOrientation:(UIDeviceOrientation)orientation {
#if TARGET_OS_SIMULATOR
    XCTSkip(@"Requires physical iPad text-field touch evidence");
#endif
    self.executionTimeAllowance = 240;
    XCUIDevice.sharedDevice.orientation = orientation;
    XCUIApplication *app = [XCUIApplication new];
    app.launchArguments = @[@"--mote-offline"];
    [self exercisePreviewNoteAtEveryPosition:app];
    XCTAttachment *tree = [XCTAttachment attachmentWithString:app.debugDescription];
    tree.name = [NSString stringWithFormat:@"physical-preview-note-%ld-role-tree",(long)orientation];
    tree.lifetime = XCTAttachmentLifetimeKeepAlways;
    [self addAttachment:tree];
    [app terminate];
}

#define PHYSICAL_GALLERY_CASE(testName, pageName, deviceOrientation, batchIndex) \
- (void)testName { [self runPhysicalGalleryPage:pageName orientation:deviceOrientation batch:batchIndex]; }

PHYSICAL_GALLERY_CASE(testPhysicalGalleryLayoutPortrait, @"Layout", UIDeviceOrientationPortrait, 0)
PHYSICAL_GALLERY_CASE(testPhysicalGalleryTogglePortrait, @"Toggle", UIDeviceOrientationPortrait, 0)
PHYSICAL_GALLERY_CASE(testPhysicalGallerySliderPortrait, @"Slider", UIDeviceOrientationPortrait, 0)
PHYSICAL_GALLERY_CASE(testPhysicalGallerySelectPortrait, @"Select", UIDeviceOrientationPortrait, 0)
PHYSICAL_GALLERY_CASE(testPhysicalGalleryListItems01Through04Portrait, @"List", UIDeviceOrientationPortrait, 0)
PHYSICAL_GALLERY_CASE(testPhysicalGalleryListItems05Through08Portrait, @"List", UIDeviceOrientationPortrait, 1)
PHYSICAL_GALLERY_CASE(testPhysicalGalleryListItems09Through12Portrait, @"List", UIDeviceOrientationPortrait, 2)
PHYSICAL_GALLERY_CASE(testPhysicalGalleryListItems13Through16Portrait, @"List", UIDeviceOrientationPortrait, 3)
PHYSICAL_GALLERY_CASE(testPhysicalGalleryListItems17Through20Portrait, @"List", UIDeviceOrientationPortrait, 4)
PHYSICAL_GALLERY_CASE(testPhysicalGalleryFormPortrait, @"Form", UIDeviceOrientationPortrait, 0)
PHYSICAL_GALLERY_CASE(testPhysicalGalleryDashboardBasePortrait, @"Dashboard", UIDeviceOrientationPortrait, 0)
PHYSICAL_GALLERY_CASE(testPhysicalGalleryDashboardTalkModelPortrait, @"Dashboard", UIDeviceOrientationPortrait, 1)
PHYSICAL_GALLERY_CASE(testPhysicalGalleryDashboardRouterModelPortrait, @"Dashboard", UIDeviceOrientationPortrait, 2)
PHYSICAL_GALLERY_CASE(testPhysicalGalleryDashboardThinkModelPortrait, @"Dashboard", UIDeviceOrientationPortrait, 3)
PHYSICAL_GALLERY_CASE(testPhysicalGalleryFixturePortrait, @"Fixture", UIDeviceOrientationPortrait, 0)
PHYSICAL_GALLERY_CASE(testPhysicalGalleryLayoutLandscape, @"Layout", UIDeviceOrientationLandscapeLeft, 0)
PHYSICAL_GALLERY_CASE(testPhysicalGalleryToggleLandscape, @"Toggle", UIDeviceOrientationLandscapeLeft, 0)
PHYSICAL_GALLERY_CASE(testPhysicalGallerySliderLandscape, @"Slider", UIDeviceOrientationLandscapeLeft, 0)
PHYSICAL_GALLERY_CASE(testPhysicalGallerySelectLandscape, @"Select", UIDeviceOrientationLandscapeLeft, 0)
PHYSICAL_GALLERY_CASE(testPhysicalGalleryListItems01Through04Landscape, @"List", UIDeviceOrientationLandscapeLeft, 0)
PHYSICAL_GALLERY_CASE(testPhysicalGalleryListItems05Through08Landscape, @"List", UIDeviceOrientationLandscapeLeft, 1)
PHYSICAL_GALLERY_CASE(testPhysicalGalleryListItems09Through12Landscape, @"List", UIDeviceOrientationLandscapeLeft, 2)
PHYSICAL_GALLERY_CASE(testPhysicalGalleryListItems13Through16Landscape, @"List", UIDeviceOrientationLandscapeLeft, 3)
PHYSICAL_GALLERY_CASE(testPhysicalGalleryListItems17Through20Landscape, @"List", UIDeviceOrientationLandscapeLeft, 4)
PHYSICAL_GALLERY_CASE(testPhysicalGalleryFormLandscape, @"Form", UIDeviceOrientationLandscapeLeft, 0)
PHYSICAL_GALLERY_CASE(testPhysicalGalleryDashboardBaseLandscape, @"Dashboard", UIDeviceOrientationLandscapeLeft, 0)
PHYSICAL_GALLERY_CASE(testPhysicalGalleryDashboardTalkModelLandscape, @"Dashboard", UIDeviceOrientationLandscapeLeft, 1)
PHYSICAL_GALLERY_CASE(testPhysicalGalleryDashboardRouterModelLandscape, @"Dashboard", UIDeviceOrientationLandscapeLeft, 2)
PHYSICAL_GALLERY_CASE(testPhysicalGalleryDashboardThinkModelLandscape, @"Dashboard", UIDeviceOrientationLandscapeLeft, 3)
PHYSICAL_GALLERY_CASE(testPhysicalGalleryFixtureLandscape, @"Fixture", UIDeviceOrientationLandscapeLeft, 0)

#undef PHYSICAL_GALLERY_CASE

- (void)testPhysicalGalleryPreviewNotePortrait {
    [self runPhysicalPreviewNoteOrientation:UIDeviceOrientationPortrait];
}

- (void)testPhysicalGalleryPreviewNoteLandscape {
    [self runPhysicalPreviewNoteOrientation:UIDeviceOrientationLandscapeLeft];
}

- (NSArray<XCUIElement *> *)orderedVoiceOverElements:(XCUIApplication *)app {
    NSPredicate *actionable = [NSPredicate predicateWithFormat:
        @"label != '' AND (elementType == %lu OR elementType == %lu OR elementType == %lu)",
        (unsigned long)XCUIElementTypeButton, (unsigned long)XCUIElementTypeOther,
        (unsigned long)XCUIElementTypeTextField];
    NSMutableArray<XCUIElement *> *elements = [NSMutableArray array];
    for (XCUIElement *element in
         [[app descendantsMatchingType:XCUIElementTypeAny]
             matchingPredicate:actionable].allElementsBoundByAccessibilityElement) {
        if (element.exists && element.frame.size.width > 0 && element.frame.size.height > 0) {
            [elements addObject:element];
        }
    }
    NSArray<NSString *> *labels = [elements valueForKey:@"label"];
    XCTAssertEqual(labels.count, [NSSet setWithArray:labels].count,
        @"VoiceOver exposed duplicate actionable labels: %@", labels);
    return elements;
}

- (void)focusVoiceOverElement:(XCUIElement *)element app:(XCUIApplication *)app {
    CGRect windowFrame = app.windows.firstMatch.frame;
    CGRect elementFrame = element.frame;
    CGVector point = CGVectorMake(
        (CGRectGetMidX(elementFrame) - CGRectGetMinX(windowFrame)) / CGRectGetWidth(windowFrame),
        (CGRectGetMidY(elementFrame) - CGRectGetMinY(windowFrame)) / CGRectGetHeight(windowFrame));
    [[app.windows.firstMatch coordinateWithNormalizedOffset:point] tap];
}

- (void)captureVoiceOverScreen:(NSString *)name {
    XCTAttachment *capture = [XCTAttachment attachmentWithScreenshot:XCUIScreen.mainScreen.screenshot];
    capture.name = name;
    capture.lifetime = XCTAttachmentLifetimeKeepAlways;
    [self addAttachment:capture];
}

- (void)fillWindowIfNeeded:(XCUIApplication *)app springboard:(XCUIApplication *)springboard {
    if (app.windows.firstMatch.frame.size.width > 700) {
        return;
    }
    XCUIElement *windowControls = springboard.buttons[@"Window Controls, Mote"];
    XCTAssertTrue([windowControls waitForExistenceWithTimeout:5],
        @"Mote started tiled but its native window controls were unavailable");
    [windowControls pressForDuration:1.0];
    XCUIElement *fill = springboard.buttons[@"Fill"];
    XCTAssertTrue([fill waitForExistenceWithTimeout:2]);
    [fill tap];
    NSPredicate *filled = [NSPredicate predicateWithBlock:^BOOL(id object, NSDictionary *bindings) {
        return app.windows.firstMatch.frame.size.width > 700;
    }];
    XCTNSPredicateExpectation *expectation =
        [[XCTNSPredicateExpectation alloc] initWithPredicate:filled object:app];
    XCTAssertEqual([XCTWaiter waitForExpectations:@[expectation] timeout:10],
        XCTWaiterResultCompleted);
}

- (void)attachPhysicalState:(NSString *)name app:(XCUIApplication *)app details:(NSDictionary *)details {
    [self capture:name app:app];
    XCTAttachment *tree = [XCTAttachment attachmentWithString:app.debugDescription];
    tree.name = [name stringByAppendingString:@"-role-tree"];
    tree.lifetime = XCTAttachmentLifetimeKeepAlways;
    [self addAttachment:tree];
    XCTAttachment *state = [XCTAttachment attachmentWithString:details.description];
    state.name = [name stringByAppendingString:@"-observations"];
    state.lifetime = XCTAttachmentLifetimeKeepAlways;
    [self addAttachment:state];
}

- (void)testPhysicalVoiceOverNamesReadingOrderAndActions {
#if TARGET_OS_SIMULATOR
    XCTSkip(@"Requires physical VoiceOver navigation");
#endif
    self.executionTimeAllowance = 600;
    XCTAssertTrue(UIAccessibilityIsVoiceOverRunning(), @"Enable physical iPad VoiceOver before running this selector");
    XCUIApplication *springboard =
        [[XCUIApplication alloc] initWithBundleIdentifier:@"com.apple.springboard"];
    XCUIElement *useVoiceOver = springboard.buttons[@"Use VoiceOver"];
    if ([useVoiceOver waitForExistenceWithTimeout:2]) {
        [useVoiceOver tap];
        NSPredicate *dismissed = [NSPredicate predicateWithFormat:@"exists == NO"];
        XCTNSPredicateExpectation *dismissal =
            [[XCTNSPredicateExpectation alloc] initWithPredicate:dismissed object:useVoiceOver];
        XCTAssertEqual([XCTWaiter waitForExpectations:@[dismissal] timeout:10],
            XCTWaiterResultCompleted);
    }
    XCUIElement *denyNotifications = springboard.buttons[@"Don’t Allow"];
    if ([denyNotifications waitForExistenceWithTimeout:2]) {
        [denyNotifications tap];
        NSPredicate *dismissed = [NSPredicate predicateWithFormat:@"exists == NO"];
        XCTNSPredicateExpectation *dismissal =
            [[XCTNSPredicateExpectation alloc] initWithPredicate:dismissed object:denyNotifications];
        XCTAssertEqual([XCTWaiter waitForExpectations:@[dismissal] timeout:10],
            XCTWaiterResultCompleted);
    }
    XCUIApplication *app = [XCUIApplication new];
    NSArray<NSString *> *navigation = galleryPages();
    for (NSString *page in navigation) {
        [app terminate];
        app.launchArguments = @[@"--mote-offline", @"--mote-page", page.lowercaseString];
        [app launch];
        XCTAssertTrue([app waitForState:XCUIApplicationStateRunningForeground timeout:15]);
        [self fillWindowIfNeeded:app springboard:springboard];
        NSArray<NSString *> *labels = [[self orderedVoiceOverElements:app] valueForKey:@"label"];
        NSArray<NSString *> *controlLabels = galleryVoiceOverControlOrder(page);
        if ([page isEqualToString:@"List"]) {
            controlLabels = [controlLabels subarrayWithRange:NSMakeRange(0, 3)];
        }
        XCTAssertTrue(galleryLabelsContainOrderedSubsequence(labels, navigation));
        XCTAssertTrue(galleryLabelsContainOrderedSubsequence(labels, controlLabels),
            @"%@ VoiceOver control order changed: %@", page, labels);
        [self attachPhysicalState:[@"physical-voiceover-" stringByAppendingString:page.lowercaseString]
            app:app details:@{@"voiceover_running":@YES, @"ordered_labels":labels}];
    }

    [app terminate];
    app.launchArguments = @[@"--mote-offline", @"--mote-page", @"toggle"];
    [app launch];
    XCUIElement *toggle = app.buttons[@"Enable preview"];
    XCTAssertTrue([toggle waitForExistenceWithTimeout:5]);
    NSString *toggleBefore = toggle.value;
    [self focusVoiceOverElement:toggle app:app];
    [self captureVoiceOverScreen:@"physical-voiceover-toggle-focused"];
    [app doubleTap];
    XCTAssertNotEqualObjects(toggle.value, toggleBefore);

    [app terminate];
    app.launchArguments = @[@"--mote-offline", @"--mote-page", @"slider"];
    [app launch];
    XCUIElement *slider = app.otherElements[@"Volume"];
    XCTAssertTrue([slider waitForExistenceWithTimeout:5]);
    NSString *sliderBefore = slider.value;
    [self focusVoiceOverElement:slider app:app];
    [self captureVoiceOverScreen:@"physical-voiceover-slider-focused"];
    [slider swipeUp];
    XCTAssertNotEqualObjects(slider.value, sliderBefore);

    [app terminate];
    app.launchArguments = @[@"--mote-offline", @"--mote-page", @"layout"];
    [app launch];
    XCTAssertTrue([app waitForState:XCUIApplicationStateRunningForeground timeout:15]);
    [self focusVoiceOverElement:app.buttons[navigation.firstObject] app:app];
    for (NSUInteger index = 0; index < navigation.count; index++) {
        [self captureVoiceOverScreen:
            [@"physical-voiceover-focus-" stringByAppendingString:navigation[index].lowercaseString]];
        if (index + 1 < navigation.count) {
            [app swipeRight];
        }
    }
    [self attachPhysicalState:@"physical-voiceover-focus-journey" app:app
        details:@{@"voiceover_running":@YES, @"accessibility_order":navigation,
            @"gesture":@"single tap Layout followed by seven one-finger right swipes"}];
    [app terminate];
}

- (void)testPhysicalSplitViewGalleryRemainsUsable {
#if TARGET_OS_SIMULATOR
    XCTSkip(@"Requires physical iPad Split View");
#endif
    self.executionTimeAllowance = 600;
    XCUIApplication *app = [XCUIApplication new];
    app.launchArguments = @[@"--mote-offline"];
    [app launch];
    XCTAssertTrue([app waitForState:XCUIApplicationStateRunningForeground timeout:15]);
    XCUIApplication *springboard = [[XCUIApplication alloc] initWithBundleIdentifier:@"com.apple.springboard"];
    CGRect fullFrame = app.windows.firstMatch.frame;
    [self fillWindowIfNeeded:app springboard:springboard];
    fullFrame = app.windows.firstMatch.frame;
    XCTAssertGreaterThan(fullFrame.size.width, 700);

    XCUICoordinate *dockStart = [app.windows.firstMatch coordinateWithNormalizedOffset:CGVectorMake(0.5, 0.995)];
    XCUICoordinate *dockEnd = [app.windows.firstMatch coordinateWithNormalizedOffset:CGVectorMake(0.5, 0.86)];
    [dockStart pressForDuration:0.05 thenDragToCoordinate:dockEnd];
    XCUIElement *safari = springboard.icons[@"Safari"];
    XCTAssertTrue([safari waitForExistenceWithTimeout:10], @"Safari must be available in the iPad Dock for Split View automation");
    XCUICoordinate *rightEdge = [springboard.windows.firstMatch coordinateWithNormalizedOffset:CGVectorMake(0.98, 0.5)];
    [[safari coordinateWithNormalizedOffset:CGVectorMake(0.5, 0.5)]
        pressForDuration:0.55 thenDragToCoordinate:rightEdge
        withVelocity:XCUIGestureVelocitySlow thenHoldForDuration:1.0];

    XCUIApplication *safariApp = [[XCUIApplication alloc] initWithBundleIdentifier:@"com.apple.mobilesafari"];
    [NSThread sleepForTimeInterval:2];
    XCTAssertTrue([safariApp.windows.firstMatch waitForExistenceWithTimeout:5],
        @"Safari did not expose a window after opening from the Dock");
    CGRect firstSafariFrame = safariApp.windows.firstMatch.frame;
    CGRect firstMoteFrame = app.windows.firstMatch.frame;
    XCTAttachment *initialScreen = [XCTAttachment attachmentWithScreenshot:XCUIScreen.mainScreen.screenshot];
    initialScreen.name = @"physical-split-initial-screen";
    initialScreen.lifetime = XCTAttachmentLifetimeKeepAlways;
    [self addAttachment:initialScreen];
    XCUIElement *windowControls = springboard.buttons[@"Window Controls, Safari"];
    if (firstMoteFrame.size.width > fullFrame.size.width * 0.75 &&
        [windowControls waitForExistenceWithTimeout:2]) {
        XCTAttachment *zoomElement = [XCTAttachment attachmentWithString:windowControls.debugDescription];
        zoomElement.name = @"physical-split-window-controls";
        zoomElement.lifetime = XCTAttachmentLifetimeKeepAlways;
        [self addAttachment:zoomElement];
        [windowControls pressForDuration:1.0];
        [NSThread sleepForTimeInterval:1];
        NSMutableArray<NSDictionary *> *buttonInventory = [NSMutableArray array];
        for (XCUIElement *button in springboard.buttons.allElementsBoundByAccessibilityElement) {
            [buttonInventory addObject:@{
                @"identifier":button.identifier ?: @"",
                @"label":button.label ?: @"",
                @"frame":NSStringFromCGRect(button.frame),
                @"hittable":@(button.hittable)}];
        }
        XCTAttachment *menuButtons = [XCTAttachment attachmentWithString:buttonInventory.description];
        menuButtons.name = @"physical-split-window-menu-buttons";
        menuButtons.lifetime = XCTAttachmentLifetimeKeepAlways;
        [self addAttachment:menuButtons];
        XCUIElement *leftAndRight = springboard.buttons[@"Left and Right"];
        XCTAssertTrue([leftAndRight waitForExistenceWithTimeout:2],
            @"The iPad window layout menu did not expose Left and Right");
        [leftAndRight tap];
        [NSThread sleepForTimeInterval:3];
    }
    XCTAttachment *controlsScreen = [XCTAttachment attachmentWithScreenshot:XCUIScreen.mainScreen.screenshot];
    controlsScreen.name = @"physical-split-window-controls-screen";
    controlsScreen.lifetime = XCTAttachmentLifetimeKeepAlways;
    [self addAttachment:controlsScreen];
    NSPredicate *narrowed = [NSPredicate predicateWithBlock:^BOOL(id object, NSDictionary *bindings) {
        CGRect frame = app.windows.firstMatch.frame;
        return frame.size.width > 0 && frame.size.width <= fullFrame.size.width * 0.75;
    }];
    XCTNSPredicateExpectation *split = [[XCTNSPredicateExpectation alloc] initWithPredicate:narrowed object:app];
    XCTAssertEqual([XCTWaiter waitForExpectations:@[split] timeout:20], XCTWaiterResultCompleted,
        @"Mote never entered a materially narrower Split View window. Full %@ current %@",
        NSStringFromCGRect(fullFrame), NSStringFromCGRect(app.windows.firstMatch.frame));
    CGRect splitFrame = app.windows.firstMatch.frame;
    CGRect splitSafariFrame = safariApp.windows.firstMatch.frame;
    XCTAssertGreaterThan(splitSafariFrame.size.width, 0);
    XCTAssertLessThanOrEqual(splitSafariFrame.size.width, fullFrame.size.width * 0.75);
    XCTAssertLessThanOrEqual(CGRectGetWidth(CGRectIntersection(splitFrame, splitSafariFrame)), 1,
        @"Mote and Safari remained overlapping instead of tiled: Mote %@ Safari %@",
        NSStringFromCGRect(splitFrame), NSStringFromCGRect(splitSafariFrame));

    for (NSString *page in galleryPages()) {
        XCUIElement *button = app.buttons[page];
        XCTAssertTrue([button waitForExistenceWithTimeout:5]);
        [button tap];
        [self assertText:[@"Selected " stringByAppendingString:page] app:app];
        if ([page isEqualToString:@"List"]) {
            XCTAssertTrue([self galleryElement:@"Open draft" kind:@"action" app:app].exists);
        } else {
            [self assertObservedGalleryInventory:page app:app];
        }
    }
    [app.buttons[@"Toggle"] tap];
    XCUIElement *toggle = [self galleryElement:@"Enable preview" kind:@"toggle" app:app];
    NSString *value = toggle.value;
    [toggle tap];
    XCTAssertNotEqualObjects(toggle.value, value);
    [app.buttons[@"Select"] tap];
    [app.buttons[@"Priority"] tap];
    XCTAssertTrue([app.buttons[@"High"] waitForExistenceWithTimeout:5]);
    [app.buttons[@"High"] tap];
    XCTAssertEqualObjects(app.buttons[@"Priority"].value, @"High");
    [app.buttons[@"List"] tap];
    XCUIElement *lastItem = [self galleryElement:@"Open item 20" kind:@"action" app:app];
    for (NSUInteger attempt = 0; attempt < 10 && !lastItem.exists; attempt++) {
        XCUICoordinate *start = [app.windows.firstMatch
            coordinateWithNormalizedOffset:CGVectorMake(0.85, 0.72)];
        XCUICoordinate *end = [app.windows.firstMatch
            coordinateWithNormalizedOffset:CGVectorMake(0.85, 0.2)];
        [start pressForDuration:0.05 thenDragToCoordinate:end];
    }
    XCTAssertTrue([lastItem waitForExistenceWithTimeout:5]);
    [lastItem tap];
    [self assertText:@"Action: gallery.item.20" app:app];
    [self attachPhysicalState:@"physical-split-view-gallery" app:app details:@{
        @"full_frame":NSStringFromCGRect(fullFrame), @"split_frame":NSStringFromCGRect(splitFrame),
        @"first_safari_frame":NSStringFromCGRect(firstSafariFrame),
        @"first_mote_frame":NSStringFromCGRect(firstMoteFrame),
        @"split_safari_frame":NSStringFromCGRect(splitSafariFrame),
        @"safari_visible":@(springboard.icons[@"Safari"].exists)}];

    [safariApp terminate];
    [app activate];
    [app terminate];
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

- (XCUIScreenshot *)captureSystemSettingsEvidence:(NSString *)name app:(XCUIApplication *)app {
    XCUIScreenshot *screenshot = app.windows.firstMatch.screenshot;
    XCTAttachment *image = [XCTAttachment attachmentWithScreenshot:screenshot];
    image.name = name;
    image.lifetime = XCTAttachmentLifetimeKeepAlways;
    [self addAttachment:image];
    XCTAttachment *tree = [XCTAttachment attachmentWithString:app.debugDescription];
    tree.name = [name stringByAppendingString:@"-role-tree"];
    tree.lifetime = XCTAttachmentLifetimeKeepAlways;
    [self addAttachment:tree];
    NSDictionary *observed = @{@"non_authoritative_xctest_appearance":@(XCUIDevice.sharedDevice.appearance),
        @"system_appearance_precondition":@"Driver retains devicectl appearance query; selector asserts visible palette",
        @"runner_reduce_motion":@(mote_ios_reduce_motion_enabled()), @"launch_arguments":app.launchArguments,
        @"target_process_motion_mode":@"Not exposed by public XCTest"};
    XCTAttachment *settings = [XCTAttachment attachmentWithString:observed.description];
    settings.name = [name stringByAppendingString:@"-system-observations"];
    settings.lifetime = XCTAttachmentLifetimeKeepAlways;
    [self addAttachment:settings];
    return screenshot;
}

- (void)runPhysicalSystemAppearance:(XCUIDeviceAppearance)expected {
#if TARGET_OS_SIMULATOR
    XCTSkip(@"Requires physical system appearance evidence");
#endif
    self.executionTimeAllowance = 180;
    XCTAssertTrue(expected == XCUIDeviceAppearanceLight || expected == XCUIDeviceAppearanceDark);
    XCUIApplication *app = [XCUIApplication new];
    app.launchArguments = @[@"--mote-offline"];
    [app launch];
    XCTAssertTrue([app waitForState:XCUIApplicationStateRunningForeground timeout:15]);
    [self selectGalleryPage:@"Layout" position:0 app:app];
    [self assertObservedGalleryInventory:@"Layout" app:app];
    NSString *name = expected == XCUIDeviceAppearanceDark ? @"system-appearance-dark" : @"system-appearance-light";
    XCUIScreenshot *screenshot = [self captureSystemSettingsEvidence:name app:app];
    CGImageRef image = screenshot.image.CGImage;
    XCTAssertNotEqual(image, NULL);
    CGRect window = app.windows.firstMatch.frame;
    XCTAssertGreaterThan(window.size.width, 0);
    XCTAssertGreaterThan(window.size.height, 0);
    CGFloat scaleX = CGImageGetWidth(image) / window.size.width;
    CGFloat scaleY = CGImageGetHeight(image) / window.size.height;
    CGAffineTransform toPixels = CGAffineTransformMake(scaleX, 0, 0, scaleY,
        -window.origin.x * scaleX, -window.origin.y * scaleY);
    NSArray *surface = expected == XCUIDeviceAppearanceDark ? @[@11,@11,@12] : @[@244,@241,@234];
    NSArray *ink = expected == XCUIDeviceAppearanceDark ? @[@232,@227,@218] : @[@26,@26,@26];
    for (NSString *label in @[@"Layout",@"Preview"]) {
        CGRect frame = [self galleryElement:label kind:@"action" app:app].frame;
        NSArray *patches = @[
            [NSValue valueWithCGRect:CGRectMake(CGRectGetMinX(frame) + 6, CGRectGetMinY(frame) + 10, 4, 4)],
            [NSValue valueWithCGRect:CGRectMake(CGRectGetMaxX(frame) - 10, CGRectGetMinY(frame) + 10, 4, 4)],
            [NSValue valueWithCGRect:CGRectMake(CGRectGetMinX(frame) + 6, CGRectGetMaxY(frame) - 14, 4, 4)],
            [NSValue valueWithCGRect:CGRectMake(CGRectGetMaxX(frame) - 10, CGRectGetMaxY(frame) - 14, 4, 4)]];
        for (NSValue *patch in patches) {
            CGRect pixels = CGRectApplyAffineTransform(patch.CGRectValue, toPixels);
            double fraction = galleryColorFraction(image, pixels, [surface[0] unsignedCharValue],
                [surface[1] unsignedCharValue], [surface[2] unsignedCharValue], 4);
            NSLog(@"SYSTEM PALETTE %@ background=%f pixels=%@",label,fraction,NSStringFromCGRect(pixels));
            XCTAssertTrue(isfinite(fraction) && fraction >= 0.95, @"Wrong %@ surface color in %@",name,label);
        }
        CGRect pixels = CGRectApplyAffineTransform(CGRectInset(frame, 10, 10), toPixels);
        double fraction = galleryColorFraction(image, pixels, [ink[0] unsignedCharValue],
            [ink[1] unsignedCharValue], [ink[2] unsignedCharValue], 8);
        NSLog(@"SYSTEM PALETTE %@ ink=%f pixels=%@",label,fraction,NSStringFromCGRect(pixels));
        XCTAssertTrue(isfinite(fraction) && fraction >= 0.001, @"Expected %@ text color in %@",name,label);
    }
    [app terminate];
}

- (void)runPhysicalSystemReduceMotion:(BOOL)expected {
#if TARGET_OS_SIMULATOR
    XCTSkip(@"Requires the physical UIKit Reduce Motion preference");
#endif
    self.executionTimeAllowance = 180;
    XCTAssertEqual(mote_ios_reduce_motion_enabled(), (uint8_t)expected, @"Set the physical Reduce Motion preference before running this selector");
    XCTAssertEqual((BOOL)UIAccessibilityIsReduceMotionEnabled(), expected);
    XCUIApplication *app = [XCUIApplication new];
    app.launchArguments = @[@"--mote-offline"];
    [app launch];
    XCTAssertTrue([app waitForState:XCUIApplicationStateRunningForeground timeout:15]);
    [self selectGalleryPage:@"Layout" position:0 app:app];
    [self assertObservedGalleryInventory:@"Layout" app:app];
    NSString *name = expected ? @"system-reduce-motion-enabled" : @"system-reduce-motion-disabled";
    [self captureSystemSettingsEvidence:[name stringByAppendingString:@"-layout"] app:app];
    XCTAssertEqual(mote_ios_reduce_motion_enabled(), (uint8_t)expected);
    [self selectGalleryPage:@"Toggle" position:0 app:app];
    [self assertObservedGalleryInventory:@"Toggle" app:app];
    [self captureSystemSettingsEvidence:[name stringByAppendingString:@"-replacement"] app:app];
    XCUIElement *toggle = [self galleryElement:@"Enable preview" kind:@"toggle" app:app];
    NSArray *values = @[@"On",@"Off"];
    XCTAssertTrue([values containsObject:toggle.value]);
    NSString *changed = [toggle.value isEqual:@"On"] ? @"Off" : @"On";
    [self tapGalleryElement:toggle position:0 app:app];
    [self assertValue:changed element:toggle];
    [self captureSystemSettingsEvidence:[name stringByAppendingString:@"-fresh-control-effect"] app:app];
    XCTAssertEqual(mote_ios_reduce_motion_enabled(), (uint8_t)expected);
    XCTAssertEqual((BOOL)UIAccessibilityIsReduceMotionEnabled(), expected);
    XCTAssertEqualObjects(app.launchArguments, (@[@"--mote-offline"]));
    [app terminate];
}

- (void)testPhysicalSystemAppearanceLight {
    [self runPhysicalSystemAppearance:XCUIDeviceAppearanceLight];
}

- (void)testPhysicalSystemAppearanceDark {
    [self runPhysicalSystemAppearance:XCUIDeviceAppearanceDark];
}

- (void)testPhysicalSystemReduceMotionEnabled {
    [self runPhysicalSystemReduceMotion:YES];
}

- (void)testPhysicalSystemReduceMotionDisabled {
    [self runPhysicalSystemReduceMotion:NO];
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
    CGRect sliderLabel = [self findTextBox:@"Volume" app:app];
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

- (void)testTextTargetCombinesAdjacentFragmentsInOneRow {
    NSArray *fragments = @[
        @{@"text": @"Open item", @"box": [NSValue valueWithCGRect:CGRectMake(0.10, 0.50, 0.20, 0.03)]},
        @{@"text": @"20", @"box": [NSValue valueWithCGRect:CGRectMake(0.31, 0.50, 0.04, 0.03)]},
        @{@"text": @"Unrelated", @"box": [NSValue valueWithCGRect:CGRectMake(0.50, 0.40, 0.20, 0.03)]}
    ];
    CGRect box = textTargetBox(fragments, @"Open item 20", 1000);
    XCTAssertFalse(CGRectIsNull(box));
    XCTAssertEqualWithAccuracy(CGRectGetMinX(box), 0.10, 0.001);
    XCTAssertEqualWithAccuracy(CGRectGetMaxX(box), 0.35, 0.001);
    XCTAssertTrue(CGRectIsNull(textTargetBox(fragments, @"Open item 21", 1000)));
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
    XCTAssertTrue([app.buttons[@"Home"] waitForExistenceWithTimeout:10]);
    XCTAssertTrue([[self nativeElement:@"Home: online" app:app] exists]);
    XCTAssertTrue([[self nativeElement:@"Message your Mote host" app:app] exists]);
    XCTAssertTrue([app.buttons[@"Send"] exists]);
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
    XCTAssertTrue([[self nativeElement:@"User: hello" app:app] exists]);
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
    XCTAssertTrue([app.buttons[@"Office"] exists]);
    XCTAssertTrue([[self nativeElement:@"Office: online" app:app] exists]);
    [self assertText:@"Office dashboard" app:app];
    XCTAssertFalse([compactText([self visibleText:app]) containsString:compactText(@"User: hello")]);
    [self capture:@"live-office-isolated" app:app];
    [self tapText:@"Home" app:app];
    [self assertText:@"Home dashboard" app:app];
    [self assertText:@"User: hello" app:app];
    XCTAssertFalse([compactText([self visibleText:app]) containsString:compactText(@"Office dashboard")]);
    [self capture:@"live-home-restored" app:app];
}

- (XCUIElement *)ensurePhysicalHostOnline:(XCUIApplication *)app {
    NSPredicate *onlinePredicate = [NSPredicate predicateWithFormat:@"label ENDSWITH ': online'"];
    XCUIElement *online = [[[app descendantsMatchingType:XCUIElementTypeAny]
        matchingPredicate:onlinePredicate] firstMatch];
    if (![online waitForExistenceWithTimeout:5]) {
        NSString *pairingCode = NSProcessInfo.processInfo.environment[@"MOTE_PHYSICAL_PAIRING_CODE"];
        XCTAssertTrue(pairingCode.length > 0, @"Set MOTE_PHYSICAL_PAIRING_CODE for a fresh physical install");
        XCUIApplication *springboard = [[XCUIApplication alloc] initWithBundleIdentifier:@"com.apple.springboard"];
        XCUIElement *allow = springboard.buttons[@"Allow"];
        if ([allow waitForExistenceWithTimeout:3]) {
            XCTAttachment *prompt = [XCTAttachment attachmentWithScreenshot:XCUIScreen.mainScreen.screenshot];
            prompt.name = @"physical-local-network-permission";
            prompt.lifetime = XCTAttachmentLifetimeKeepAlways;
            [self addAttachment:prompt];
            [allow tap];
            XCTAssertTrue([app.windows.firstMatch waitForExistenceWithTimeout:10]);
            NSLog(@"LOCAL NETWORK PERMISSION: visible purpose text and Allow accepted");
        }
        XCUIElement *discovered = [self nativeElement:@"Select a host and enter its MOTE1 pairing code" app:app];
        if (![discovered waitForExistenceWithTimeout:8]) {
            [self tapText:@"Refresh" app:app];
            XCTAssertTrue([discovered waitForExistenceWithTimeout:15]);
        }
        NSPredicate *wifiHostPredicate = [NSPredicate predicateWithFormat:
            @"label BEGINSWITH 'Mote host ' AND NOT label CONTAINS '169.254.'"];
        XCUIElement *wifiHost = [[[app descendantsMatchingType:XCUIElementTypeButton]
            matchingPredicate:wifiHostPredicate] firstMatch];
        XCTAssertTrue([wifiHost waitForExistenceWithTimeout:5]);
        [wifiHost tap];
        XCUIElement *pairingControl = [self nativeElement:@"MOTE1 pairing code" app:app];
        XCTAssertTrue([pairingControl waitForExistenceWithTimeout:5]);
        [pairingControl tap];
        XCTAssertTrue([app.keyboards.firstMatch waitForExistenceWithTimeout:5]);
        XCUIElement *pairingField = app.secureTextFields[@"mote.native-text-input"];
        XCTAssertTrue([pairingField waitForExistenceWithTimeout:5]);
        [pairingField typeText:pairingCode];
        XCUIElement *pair = app.buttons[@"Pair"];
        XCTAssertTrue([pair waitForExistenceWithTimeout:5]);
        XCTAssertTrue(pair.enabled);
        [pair tap];
        NSLog(@"PAIRING COORDINATION: confirm the waiting physical iPad now");
        XCTAssertTrue([online waitForExistenceWithTimeout:180]);
    }
    return online;
}

- (void)testPhysicalPairedHostDisconnectsAndReconnectsWithoutRepairing {
#if TARGET_OS_SIMULATOR
    XCTSkip(@"Requires a paired physical iPad");
#endif
    XCUIApplication *app = [[XCUIApplication alloc] init];
    [app activate];
    XCTAssertTrue([app waitForState:XCUIApplicationStateRunningForeground timeout:15]);

    XCUIElement *online = [self ensurePhysicalHostOnline:app];
    NSString *onlineLabel = online.label;
    NSString *prefix = [onlineLabel substringToIndex:onlineLabel.length - @"online".length];
    XCUIElement *cursor = [[[app descendantsMatchingType:XCUIElementTypeAny]
        matchingPredicate:[NSPredicate predicateWithFormat:@"label BEGINSWITH 'Cursor: '"]] firstMatch];
    XCTAssertTrue([cursor waitForExistenceWithTimeout:10]);
    NSString *cursorLabel = cursor.label;
    [self capture:@"physical-wifi-online-before" app:app];

    NSString *offlineLabel = [prefix stringByAppendingString:@"offline"];
    NSString *connectingLabel = [prefix stringByAppendingString:@"connecting"];
    NSPredicate *disconnected = [NSPredicate predicateWithBlock:^BOOL(id object, NSDictionary *bindings) {
        return [self nativeElement:offlineLabel app:app].exists ||
               [self nativeElement:connectingLabel app:app].exists;
    }];
    XCTNSPredicateExpectation *disconnectExpectation =
        [[XCTNSPredicateExpectation alloc] initWithPredicate:disconnected object:app];
    XCTAssertEqual([XCTWaiter waitForExpectations:@[disconnectExpectation] timeout:120],
                   XCTWaiterResultCompleted);
    [self capture:@"physical-wifi-disconnected" app:app];

    XCUIElement *reconnected = [self nativeElement:onlineLabel app:app];
    XCTAssertTrue([reconnected waitForExistenceWithTimeout:180]);
    XCTAssertTrue([self nativeElement:cursorLabel app:app].exists);
    [self capture:@"physical-wifi-reconnected" app:app];
}

- (void)testPhysicalInstallMoteFromInternalTestFlight {
#if TARGET_OS_SIMULATOR
    XCTSkip(@"Requires a physical iPad with the owner's TestFlight account");
#endif
    self.executionTimeAllowance = 600;
    XCUIApplication *testFlight =
        [[XCUIApplication alloc] initWithBundleIdentifier:@"com.apple.TestFlight"];
    [testFlight launch];
    XCTAssertTrue([testFlight waitForState:XCUIApplicationStateRunningForeground timeout:30]);

    XCUIElement *continueButton = testFlight.buttons[@"Continue"];
    if ([continueButton waitForExistenceWithTimeout:2]) {
        [continueButton tap];
    }

    XCUIElement *mote = [[[testFlight descendantsMatchingType:XCUIElementTypeAny]
        matchingPredicate:[NSPredicate predicateWithFormat:@"label == 'Mote Agent'"]] firstMatch];
    if (![mote waitForExistenceWithTimeout:2]) {
        XCUIElement *sidebar = testFlight.buttons[@"Show Sidebar"];
        XCTAssertTrue([sidebar waitForExistenceWithTimeout:5]);
        [sidebar tap];
    }
    if (![mote waitForExistenceWithTimeout:20]) {
        [self attachPhysicalState:@"testflight-mote-unavailable" app:testFlight
            details:@{@"expected_app":@"Mote Agent"}];
        XCTFail(@"The signed-in TestFlight account does not expose Mote Agent");
        return;
    }
    [mote tap];
    XCUIElementQuery *hideSidebarButtons = [testFlight.buttons
        matchingPredicate:[NSPredicate predicateWithFormat:@"label == 'Hide Sidebar'"]];
    if ([hideSidebarButtons.firstMatch waitForExistenceWithTimeout:2]) {
        for (XCUIElement *button in hideSidebarButtons.allElementsBoundByIndex) {
            if (button.hittable) {
                [button tap];
                break;
            }
        }
    }

    XCUIElement *open = testFlight.buttons[@"TestFlight.offerButton.open"];
    if (![open waitForExistenceWithTimeout:3]) {
        BOOL installationStarted = NO;
        XCUIElement *replaceAlert = [[testFlight.alerts
            matchingPredicate:[NSPredicate predicateWithFormat:
                @"label == 'You already have this app installed.'"]] firstMatch];
        if ([replaceAlert waitForExistenceWithTimeout:2]) {
            XCUIElement *confirmInstall = replaceAlert.buttons[@"Install"];
            XCTAssertTrue([confirmInstall waitForExistenceWithTimeout:2]);
            [confirmInstall tap];
            installationStarted = YES;
        }
        XCUIElement *install = testFlight.buttons[@"TestFlight.offerButton.install"];
        if ([install waitForExistenceWithTimeout:10]) {
            [install tap];
            installationStarted = YES;
            if ([replaceAlert waitForExistenceWithTimeout:5]) {
                XCUIElement *confirmInstall = replaceAlert.buttons[@"Install"];
                XCTAssertTrue([confirmInstall waitForExistenceWithTimeout:2]);
                [confirmInstall tap];
            }
        }
        XCUIElement *inProgress = [[[testFlight descendantsMatchingType:XCUIElementTypeAny]
            matchingPredicate:[NSPredicate predicateWithFormat:@"label == 'In Progress'"]] firstMatch];
        XCTAssertTrue(installationStarted || [inProgress waitForExistenceWithTimeout:5]);
        XCTAssertTrue([open waitForExistenceWithTimeout:300]);
    }
    [self attachPhysicalState:@"testflight-mote-installed" app:testFlight
        details:@{@"app":@"Mote Agent", @"terminal_control":open.label ?: @""}];
}

- (NSDictionary *)backgroundObservationsForApp:(XCUIApplication *)app springboard:(XCUIApplication *)springboard {
    BOOL homeIcon = NO;
    for (XCUIElement *icon in springboard.icons.allElementsBoundByIndex) {
        if (icon.hittable) { homeIcon = YES; break; }
    }
    BOOL moteControl = NO;
    for (XCUIElement *element in [app descendantsMatchingType:XCUIElementTypeAny].allElementsBoundByIndex) {
        if (element.hittable) { moteControl = YES; break; }
    }
    return @{@"springboard_visible":@(springboard.exists && springboard.windows.firstMatch.exists),
        @"home_icon_hittable":@(homeIcon), @"mote_surface_hittable":@(app.hittable),
        @"mote_descendant_hittable":@(moteControl), @"diagnostic_mote_state":@(app.state),
        @"diagnostic_springboard_state":@(springboard.state)};
}

- (BOOL)backgroundObservationsAreSufficient:(NSDictionary *)observations {
    return galleryBackgroundEvidenceSufficient([observations[@"springboard_visible"] boolValue],
        [observations[@"home_icon_hittable"] boolValue],
        (XCUIApplicationState)[observations[@"diagnostic_mote_state"] integerValue]);
}

- (NSArray *)moveToHomeAndObserveApp:(XCUIApplication *)app springboard:(XCUIApplication *)springboard {
    [XCUIDevice.sharedDevice pressButton:XCUIDeviceButtonHome];
    [springboard activate];
    [NSThread sleepForTimeInterval:2];
    NSDictionary *observations = [self backgroundObservationsForApp:app springboard:springboard];
    NSDictionary *sample = @{ @"home_transition":@"press-and-activate", @"observations":observations };
    NSLog(@"HOME TRANSITION OBSERVATIONS %@", sample);
    return @[sample];
}

- (void)captureBackgroundEvidence:(NSString *)name app:(XCUIApplication *)app springboard:(XCUIApplication *)springboard samples:(NSArray *)samples {
    XCTAttachment *screen = [XCTAttachment attachmentWithScreenshot:XCUIScreen.mainScreen.screenshot];
    screen.name = name;
    screen.lifetime = XCTAttachmentLifetimeKeepAlways;
    [self addAttachment:screen];
    NSDictionary *trees = @{@"springboard":springboard.debugDescription, @"mote":app.debugDescription,
        @"observations":samples};
    XCTAttachment *roles = [XCTAttachment attachmentWithString:trees.description];
    roles.name = [name stringByAppendingString:@"-role-state"];
    roles.lifetime = XCTAttachmentLifetimeKeepAlways;
    [self addAttachment:roles];
}

- (void)testPhysicalPairedHostRecoversAfterBackgrounding {
#if TARGET_OS_SIMULATOR
    XCTSkip(@"Requires a paired physical iPad");
#endif
    self.executionTimeAllowance = 900;
    XCUIApplication *app = [[XCUIApplication alloc] init];
    [app activate];
    XCTAssertTrue([app waitForState:XCUIApplicationStateRunningForeground timeout:15]);

    XCUIElement *online = [self ensurePhysicalHostOnline:app];
    NSString *onlineLabel = online.label;
    XCUIElement *cursor = [[[app descendantsMatchingType:XCUIElementTypeAny]
        matchingPredicate:[NSPredicate predicateWithFormat:@"label BEGINSWITH 'Cursor: '"]] firstMatch];
    XCTAssertTrue([cursor waitForExistenceWithTimeout:10]);
    NSString *cursorLabel = cursor.label;
    NSMutableArray<NSString *> *labelsBefore = [NSMutableArray array];
    for (XCUIElement *element in [app descendantsMatchingType:XCUIElementTypeAny].allElementsBoundByIndex) {
        if (element.label.length) [labelsBefore addObject:element.label];
    }
    NSArray<NSString *> *conversationBefore = galleryStableConversationLabels(labelsBefore);
    XCTAssertGreaterThan(conversationBefore.count, (NSUInteger)0,
        @"At least one durable conversation row is required for the preservation check");
    [self capture:@"physical-background-before" app:app];
    XCTAssertTrue(app.hittable);

    XCUIApplication *springboard = [[XCUIApplication alloc] initWithBundleIdentifier:@"com.apple.springboard"];
    NSMutableArray *stateSamples = [[self moveToHomeAndObserveApp:app springboard:springboard] mutableCopy];
    NSTimeInterval backgroundStart = NSProcessInfo.processInfo.systemUptime;
    NSPredicate *backgrounded = [NSPredicate predicateWithBlock:^BOOL(id object, NSDictionary *bindings) {
        NSDictionary *observations = [self backgroundObservationsForApp:app springboard:springboard];
        NSDictionary *sample = @{@"elapsed":@(NSProcessInfo.processInfo.systemUptime - backgroundStart),
            @"observations":observations};
        [stateSamples addObject:sample];
        NSLog(@"BACKGROUND OBSERVATIONS %@", sample);
        return [self backgroundObservationsAreSufficient:observations];
    }];
    XCTNSPredicateExpectation *background = [[XCTNSPredicateExpectation alloc] initWithPredicate:backgrounded object:app];
    XCTWaiterResult backgroundResult = [XCTWaiter waitForExpectations:@[background] timeout:60];
    [self captureBackgroundEvidence:@"physical-background-home-observed" app:app springboard:springboard samples:stateSamples];
    XCTAssertEqual(backgroundResult, XCTWaiterResultCompleted, @"Visible Home and Mote background state required. Samples: %@",stateSamples);
    NSLog(@"BACKGROUND COORDINATION: Home observed and Mote background state confirmed; stop this paired Mote host within 30 seconds and keep it stopped until the offline capture");
    XCTestExpectation *stopWindow = [self expectationWithDescription:@"Driver stops host while Home is visible"];
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 30 * NSEC_PER_SEC), dispatch_get_main_queue(), ^{
        [stopWindow fulfill];
    });
    [self waitForExpectations:@[stopWindow] timeout:35];
    NSDictionary *beforeActivation = [self backgroundObservationsForApp:app springboard:springboard];
    [self captureBackgroundEvidence:@"physical-background-before-activation" app:app springboard:springboard samples:@[beforeActivation]];
    XCTAssertTrue([self backgroundObservationsAreSufficient:beforeActivation]);

    [app activate];
    XCTAssertTrue([app waitForState:XCUIApplicationStateRunningForeground timeout:20]);
    NSString *prefix = [onlineLabel substringToIndex:onlineLabel.length - @"online".length];
    NSString *offlineLabel = [prefix stringByAppendingString:@"offline"];
    XCTAssertTrue([[self nativeElement:offlineLabel app:app] waitForExistenceWithTimeout:30]);
    [self capture:@"physical-background-host-stopped" app:app];
    NSLog(@"BACKGROUND COORDINATION: offline captured; start the same paired Mote host now");
    XCTAssertTrue([[self nativeElement:onlineLabel app:app] waitForExistenceWithTimeout:180]);
    XCTAssertTrue([[self nativeElement:cursorLabel app:app] exists]);
    for (NSString *label in conversationBefore) {
        XCTAssertTrue([[self nativeElement:label app:app] exists], @"Missing preserved conversation row: %@",label);
    }
    [self capture:@"physical-background-recovered" app:app];

    NSString *marker = [@"background-recovery-" stringByAppendingString:NSUUID.UUID.UUIDString];
    [self tapText:@"Message your Mote host" app:app];
    XCTAssertTrue([app.keyboards.firstMatch waitForExistenceWithTimeout:5]);
    XCUIElement *composer = app.textFields[@"mote.native-text-input"];
    XCTAssertTrue([composer waitForExistenceWithTimeout:5]);
    [composer typeText:marker];
    [self tapText:@"Send" app:app];
    NSString *messageLabel = [@"User: " stringByAppendingString:marker];
    XCTAssertTrue([[self nativeElement:messageLabel app:app] waitForExistenceWithTimeout:30]);
    long long initialSequence = [[cursorLabel substringFromIndex:@"Cursor: ".length] longLongValue];
    NSPredicate *advanced = [NSPredicate predicateWithBlock:^BOOL(id object, NSDictionary *bindings) {
        return cursor.exists && [[cursor.label substringFromIndex:@"Cursor: ".length] longLongValue] > initialSequence;
    }];
    XCTNSPredicateExpectation *newEvent = [[XCTNSPredicateExpectation alloc] initWithPredicate:advanced object:app];
    XCTAssertEqual([XCTWaiter waitForExpectations:@[newEvent] timeout:30], XCTWaiterResultCompleted);
    long long advancedSequence = [[cursor.label substringFromIndex:@"Cursor: ".length] longLongValue];
    NSPredicate *messageMatch = [NSPredicate predicateWithFormat:@"label == %@",messageLabel];
    XCTAssertEqual([[app descendantsMatchingType:XCUIElementTypeAny] matchingPredicate:messageMatch].count, (NSUInteger)1);
    [self capture:@"physical-background-new-cursor-event" app:app];
    [app terminate];
    [app launch];
    XCTAssertTrue([[self nativeElement:onlineLabel app:app] waitForExistenceWithTimeout:30]);
    NSPredicate *replayed = [NSPredicate predicateWithBlock:^BOOL(id object, NSDictionary *bindings) {
        return cursor.exists && [[cursor.label substringFromIndex:@"Cursor: ".length] longLongValue] >= advancedSequence;
    }];
    XCTNSPredicateExpectation *replayedCursor = [[XCTNSPredicateExpectation alloc] initWithPredicate:replayed object:app];
    XCTAssertEqual([XCTWaiter waitForExpectations:@[replayedCursor] timeout:30], XCTWaiterResultCompleted);
    XCTAssertTrue([[self nativeElement:messageLabel app:app] waitForExistenceWithTimeout:30]);
    XCTAssertEqual([[app descendantsMatchingType:XCUIElementTypeAny] matchingPredicate:messageMatch].count, (NSUInteger)1);
    [self capture:@"physical-background-cursor-replay-no-duplicates" app:app];
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
    XCTAssertTrue([app.buttons[@"Approve"] waitForExistenceWithTimeout:10]);
    XCTAssertTrue([app.buttons[@"Deny"] exists]);
    XCTAssertTrue([[self nativeElement:@"Browser: online" app:app] exists]);
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
    CGRect box = [self findTextBox:@"Approve" app:app];
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
    XCTAssertFalse(app.buttons[@"Approve"].exists);
    XCTAssertFalse(app.buttons[@"Deny"].exists);
    [self assertText:@"Approved request completed." app:app];
    NSLog(@"MOTE BROWSER EXPIRED OBSERVED");
}
@end
