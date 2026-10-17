#import "mote_ios.h"
#import <UIKit/UIKit.h>
#import <TargetConditionals.h>
#import <Security/Security.h>
#include <dns_sd.h>
#include <arpa/inet.h>
#include <poll.h>
#include <math.h>

static NSDictionary *keychainQuery(const char *account) {
    return @{(__bridge id)kSecClass: (__bridge id)kSecClassGenericPassword,
             (__bridge id)kSecAttrService: @"local.mote.paired-host",
             (__bridge id)kSecAttrAccount: [NSString stringWithUTF8String:account]};
}

intptr_t mote_ios_keychain_load(const char *account, uint8_t *output, size_t capacity) {
    @autoreleasepool {
        NSMutableDictionary *query = [keychainQuery(account) mutableCopy];
        query[(__bridge id)kSecReturnData] = @YES;
        query[(__bridge id)kSecMatchLimit] = (__bridge id)kSecMatchLimitOne;
        CFTypeRef result = NULL;
        OSStatus status = SecItemCopyMatching((__bridge CFDictionaryRef)query, &result);
        if (status != errSecSuccess) return status;
        NSData *data = CFBridgingRelease(result);
        if (data.length > capacity) return errSecParam;
        memcpy(output, data.bytes, data.length);
        return data.length;
    }
}

int32_t mote_ios_keychain_save(const char *account, const uint8_t *bytes, size_t length) {
    @autoreleasepool {
        NSDictionary *query = keychainQuery(account);
        NSDictionary *attributes = @{(__bridge id)kSecValueData: [NSData dataWithBytes:bytes length:length],
                                     (__bridge id)kSecAttrAccessible: (__bridge id)kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly};
        OSStatus status = SecItemUpdate((__bridge CFDictionaryRef)query, (__bridge CFDictionaryRef)attributes);
        if (status != errSecItemNotFound) return status;
        NSMutableDictionary *item = [query mutableCopy];
        [item addEntriesFromDictionary:attributes];
        return SecItemAdd((__bridge CFDictionaryRef)item, NULL);
    }
}

int32_t mote_ios_keychain_delete(const char *account) {
    @autoreleasepool { return SecItemDelete((__bridge CFDictionaryRef)keychainQuery(account)); }
}

@interface MoteDiscovery : NSObject
@property DNSServiceRef connection;
@property(nonatomic, strong) NSMutableArray<NSValue *> *references;
@property(nonatomic, strong) NSMutableArray *contexts;
@property(nonatomic, strong) NSMutableArray<NSDictionary *> *records;
@property BOOL failed;
@end
@implementation MoteDiscovery
@end

@interface MoteResolvedService : NSObject
@property(nonatomic, weak) MoteDiscovery *browser;
@property(nonatomic, strong) NSArray<NSString *> *txt;
@property uint16_t port;
@end
@implementation MoteResolvedService
@end

static void discoveredAddress(DNSServiceRef ref, DNSServiceFlags flags, uint32_t interface,
                              DNSServiceErrorType error, const char *hostname,
                              const struct sockaddr *address, uint32_t ttl, void *context) {
    MoteResolvedService *service = (__bridge MoteResolvedService *)context;
    if (error || !(flags & kDNSServiceFlagsAdd) || address->sa_family != AF_INET) return;
    char ip[INET_ADDRSTRLEN];
    const struct sockaddr_in *ipv4 = (const struct sockaddr_in *)address;
    if (!inet_ntop(AF_INET, &ipv4->sin_addr, ip, sizeof(ip))) return;
    NSDictionary *record = @{ @"endpoint": [NSString stringWithFormat:@"%s:%u", ip, service.port], @"txt": service.txt };
    if (service.browser.records.count < 16 && ![service.browser.records containsObject:record]) {
        [service.browser.records addObject:record];
    }
}

static void resolvedService(DNSServiceRef ref, DNSServiceFlags flags, uint32_t interface,
                            DNSServiceErrorType error, const char *fullname, const char *host,
                            uint16_t port, uint16_t length, const unsigned char *txt, void *context) {
    MoteDiscovery *browser = (__bridge MoteDiscovery *)context;
    if (error || TXTRecordGetCount(length, txt) != 2) return;
    NSMutableArray *fields = [NSMutableArray array];
    for (uint16_t i = 0; i < 2; i++) {
        char key[256]; uint8_t size; const void *value;
        if (TXTRecordGetItemAtIndex(length, txt, i, sizeof(key), key, &size, &value)) return;
        NSString *text = [[NSString alloc] initWithBytes:value length:size encoding:NSUTF8StringEncoding];
        if (!text) return;
        [fields addObject:[NSString stringWithFormat:@"%s=%@", key, text]];
    }
    MoteResolvedService *service = [MoteResolvedService new];
    service.browser = browser; service.port = ntohs(port); service.txt = fields;
    [browser.contexts addObject:service];
    DNSServiceRef query = browser.connection;
    DNSServiceErrorType status = DNSServiceGetAddrInfo(&query, kDNSServiceFlagsShareConnection,
        interface, kDNSServiceProtocol_IPv4, host, discoveredAddress, (__bridge void *)service);
    if (status) { browser.failed = YES; return; }
    [browser.references addObject:[NSValue valueWithPointer:query]];
}

static void foundService(DNSServiceRef ref, DNSServiceFlags flags, uint32_t interface,
                         DNSServiceErrorType error, const char *name, const char *type,
                         const char *domain, void *context) {
    MoteDiscovery *browser = (__bridge MoteDiscovery *)context;
    if (error) { browser.failed = YES; return; }
    if (!(flags & kDNSServiceFlagsAdd) || browser.references.count >= 32) return;
    DNSServiceRef resolve = browser.connection;
    DNSServiceErrorType status = DNSServiceResolve(&resolve, kDNSServiceFlagsShareConnection,
        interface, name, type, domain, resolvedService, context);
    if (status) { browser.failed = YES; return; }
    [browser.references addObject:[NSValue valueWithPointer:resolve]];
}

intptr_t mote_ios_discover(double timeout, uint8_t *output, size_t capacity) {
    @autoreleasepool {
        MoteDiscovery *browser = [MoteDiscovery new];
        browser.references = [NSMutableArray array]; browser.contexts = [NSMutableArray array];
        browser.records = [NSMutableArray array];
        DNSServiceRef connection;
        DNSServiceErrorType status = DNSServiceCreateConnection(&connection);
        if (status) return status;
        browser.connection = connection;
        DNSServiceRef browse = connection;
        status = DNSServiceBrowse(&browse, kDNSServiceFlagsShareConnection, kDNSServiceInterfaceIndexAny,
            "_mote._tcp", "local", foundService, (__bridge void *)browser);
        if (status) { DNSServiceRefDeallocate(connection); return status; }
        [browser.references addObject:[NSValue valueWithPointer:browse]];
        NSDate *deadline = [NSDate dateWithTimeIntervalSinceNow:timeout];
        while (!browser.failed && deadline.timeIntervalSinceNow > 0) {
            struct pollfd descriptor = { DNSServiceRefSockFD(connection), POLLIN, 0 };
            int ready = poll(&descriptor, 1, MIN(100, (int)(deadline.timeIntervalSinceNow * 1000)));
            if (ready < 0 || (ready > 0 && DNSServiceProcessResult(connection))) browser.failed = YES;
        }
        for (NSValue *reference in browser.references) DNSServiceRefDeallocate(reference.pointerValue);
        DNSServiceRefDeallocate(connection);
        for (MoteResolvedService *service in browser.contexts) service.browser = nil;
        if (browser.failed) return -1;
        NSData *data = [NSJSONSerialization dataWithJSONObject:browser.records options:0 error:nil];
        if (data.length > capacity) return -1;
        memcpy(output, data.bytes, data.length);
        return data.length;
    }
}

#if TARGET_OS_SIMULATOR
const char *mote_ios_probe_documents(void) {
    static NSString *documents;
    static dispatch_once_t once;
    dispatch_once(&once, ^{
        documents = NSSearchPathForDirectoriesInDomains(NSDocumentDirectory, NSUserDomainMask, YES).firstObject;
    });
    return documents.UTF8String;
}
#endif

const char *mote_ios_documents(void) {
    static NSString *documents;
    static dispatch_once_t once;
    dispatch_once(&once, ^{ documents = NSSearchPathForDirectoriesInDomains(NSDocumentDirectory, NSUserDomainMask, YES).firstObject; });
    return documents.UTF8String;
}

@interface MoteTextInputProxy : UITextField <UITextFieldDelegate>
@property uint64_t controlID;
@property BOOL synchronizing;
@end

@implementation MoteTextInputProxy
- (instancetype)initWithFrame:(CGRect)frame {
    self = [super initWithFrame:frame];
    if (self) {
        self.delegate = self;
        self.backgroundColor = UIColor.clearColor;
        self.borderStyle = UITextBorderStyleNone;
        self.textColor = UIColor.clearColor;
        self.tintColor = [UIColor colorWithRed:0.95 green:0.12 blue:0.12 alpha:1.0];
        self.font = [UIFont systemFontOfSize:14.0];
        self.clearButtonMode = UITextFieldViewModeNever;
        self.autocorrectionType = UITextAutocorrectionTypeDefault;
        self.spellCheckingType = UITextSpellCheckingTypeDefault;
        self.smartQuotesType = UITextSmartQuotesTypeDefault;
        self.smartDashesType = UITextSmartDashesTypeDefault;
        self.autocapitalizationType = UITextAutocapitalizationTypeSentences;
        self.returnKeyType = UIReturnKeySend;
        self.isAccessibilityElement = YES;
        self.accessibilityIdentifier = @"mote.native-text-input";
        [self addTarget:self action:@selector(textChanged:) forControlEvents:UIControlEventEditingChanged];
    }
    return self;
}

- (void)notifyRust {
    if (self.synchronizing) return;
    NSData *data = [self.text dataUsingEncoding:NSUTF8StringEncoding];
    NSRange selection = [self selectedTextRange]
        ? NSMakeRange([self offsetFromPosition:self.beginningOfDocument
                                   toPosition:self.selectedTextRange.start],
                      [self offsetFromPosition:self.selectedTextRange.start
                                   toPosition:self.selectedTextRange.end])
        : NSMakeRange(self.text.length, 0);
    mote_ios_text_input_update(self.controlID, data.bytes, data.length,
                               selection.location, NSMaxRange(selection));
}

- (void)textChanged:(UITextField *)field {
    [self notifyRust];
}

- (void)textFieldDidChangeSelection:(UITextField *)textField {
    [self notifyRust];
}

- (BOOL)textFieldShouldReturn:(UITextField *)textField {
    mote_ios_text_input_return(self.controlID);
    return NO;
}
@end

static MoteTextInputProxy *MoteTextInput;
static BOOL MoteNativeInputTestEnabled;
static BOOL MoteNativeInputTestStarted;
static void refreshMoteAccessibilityElements(void);

static void failMoteNativeInputTest(NSString *reason) {
    MoteTextInput.text = [@"mote-test-failed:" stringByAppendingString:reason];
    [MoteTextInput sendActionsForControlEvents:UIControlEventEditingChanged];
}

static void startMoteNativeInputTest(void) {
    if (!MoteNativeInputTestEnabled || MoteNativeInputTestStarted ||
        !MoteTextInput.isFirstResponder) return;
    MoteNativeInputTestStarted = YES;
    [MoteTextInput setMarkedText:@"かな" selectedRange:NSMakeRange(2, 0)];
    [MoteTextInput sendActionsForControlEvents:UIControlEventEditingChanged];
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 500 * NSEC_PER_MSEC),
                   dispatch_get_main_queue(), ^{
        if (MoteTextInput.markedTextRange == nil ||
            ![MoteTextInput.text isEqualToString:@"かな"]) {
            failMoteNativeInputTest(@"marked-text-lost");
            return;
        }
        [MoteTextInput unmarkText];
        [MoteTextInput sendActionsForControlEvents:UIControlEventEditingChanged];
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 500 * NSEC_PER_MSEC),
                       dispatch_get_main_queue(), ^{
            if (MoteTextInput.markedTextRange != nil ||
                ![MoteTextInput.text isEqualToString:@"かな"]) {
                failMoteNativeInputTest(@"marked-text-not-committed");
                return;
            }
            [MoteTextInput insertText:@" Mote dictation works"];
        });
    });
}

void mote_ios_text_input_publish(const uint8_t *bytes, size_t length) {
    @autoreleasepool {
        if (!MoteTextInput) return;
        if (!bytes || length == 0) {
            [MoteTextInput resignFirstResponder];
            MoteTextInput.hidden = YES;
            MoteTextInput.userInteractionEnabled = NO;
            refreshMoteAccessibilityElements();
            return;
        }
        NSData *data = [NSData dataWithBytes:bytes length:length];
        id decoded = [NSJSONSerialization JSONObjectWithData:data options:0 error:nil];
        if (![decoded isKindOfClass:[NSDictionary class]]) return;
        NSDictionary *item = decoded;
        NSArray<NSNumber *> *rect = item[@"rect"];
        NSNumber *identity = item[@"id"];
        NSString *label = item[@"label"];
        NSString *text = item[@"text"];
        NSNumber *start = item[@"selection_start"];
        NSNumber *end = item[@"selection_end"];
        NSNumber *secure = item[@"secure"];
        if (![identity isKindOfClass:[NSNumber class]] ||
            ![label isKindOfClass:[NSString class]] ||
            ![text isKindOfClass:[NSString class]] ||
            ![start isKindOfClass:[NSNumber class]] ||
            ![end isKindOfClass:[NSNumber class]] ||
            ![secure isKindOfClass:[NSNumber class]] ||
            ![rect isKindOfClass:[NSArray class]] || rect.count != 4) return;
        NSUInteger lower = MIN(start.unsignedIntegerValue, text.length);
        NSUInteger upper = MIN(MAX(end.unsignedIntegerValue, lower), text.length);
        BOOL changedField = MoteTextInput.controlID != identity.unsignedLongLongValue;
        MoteTextInput.synchronizing = YES;
        MoteTextInput.controlID = identity.unsignedLongLongValue;
        MoteTextInput.accessibilityLabel = label;
        MoteTextInput.frame = CGRectMake(rect[0].doubleValue, rect[1].doubleValue,
                                         rect[2].doubleValue, rect[3].doubleValue);
        if (MoteTextInput.secureTextEntry != secure.boolValue) {
            MoteTextInput.secureTextEntry = secure.boolValue;
        }
        BOOL composing = !changedField && MoteTextInput.markedTextRange != nil;
        if (!composing) {
            if (![MoteTextInput.text isEqualToString:text]) MoteTextInput.text = text;
            UITextPosition *from = [MoteTextInput positionFromPosition:MoteTextInput.beginningOfDocument
                                                                offset:(NSInteger)lower];
            UITextPosition *to = [MoteTextInput positionFromPosition:MoteTextInput.beginningOfDocument
                                                              offset:(NSInteger)upper];
            if (from && to) {
                UITextRange *current = MoteTextInput.selectedTextRange;
                NSInteger currentLower = current
                    ? [MoteTextInput offsetFromPosition:MoteTextInput.beginningOfDocument
                                             toPosition:current.start]
                    : NSNotFound;
                NSInteger currentUpper = current
                    ? [MoteTextInput offsetFromPosition:MoteTextInput.beginningOfDocument
                                             toPosition:current.end]
                    : NSNotFound;
                if (currentLower != (NSInteger)lower || currentUpper != (NSInteger)upper) {
                    MoteTextInput.selectedTextRange =
                        [MoteTextInput textRangeFromPosition:from toPosition:to];
                }
            }
        }
        MoteTextInput.synchronizing = NO;
        MoteTextInput.hidden = NO;
        MoteTextInput.userInteractionEnabled = YES;
        if (changedField || !MoteTextInput.isFirstResponder) [MoteTextInput becomeFirstResponder];
        startMoteNativeInputTest();
        refreshMoteAccessibilityElements();
    }
}

static UIWindow *MoteWindow;
static UIWindowScene *MoteWindowScene;
static UIView *MoteAccessibilityView;
@class MoteControlElement;
static NSMutableDictionary<NSNumber *, MoteControlElement *> *MoteAccessibilityElements;
static NSArray<id> *MotePublishedAccessibilityElements;

@interface MoteControlElement : UIAccessibilityElement
@property uint64_t controlID;
@property uint8_t actionKind;
@property BOOL controlEnabled;
@property(nonatomic, copy) NSString *controlRole;
@end

static void refreshMoteAccessibilityElements(void) {
    if (!MoteAccessibilityView || !MotePublishedAccessibilityElements) return;
    if (!MoteTextInput || MoteTextInput.hidden) {
        MoteAccessibilityView.accessibilityElements = MotePublishedAccessibilityElements;
        return;
    }
    NSMutableArray<id> *filtered = [NSMutableArray array];
    for (MoteControlElement *element in MotePublishedAccessibilityElements) {
        if (element.controlID != MoteTextInput.controlID) [filtered addObject:element];
    }
    [filtered addObject:MoteTextInput];
    MoteAccessibilityView.accessibilityElements = filtered;
}

@implementation MoteControlElement
- (BOOL)accessibilityActivate {
    if (!self.controlEnabled || self.actionKind == 255) return NO;
    if (![MoteAccessibilityView.accessibilityElements containsObject:self]) return NO;
    mote_ios_accessibility_action(self.controlID, self.actionKind);
    return YES;
}
- (void)accessibilityIncrement {
    if (self.controlEnabled && self.actionKind == 1 &&
        [MoteAccessibilityView.accessibilityElements containsObject:self])
        mote_ios_accessibility_action(self.controlID, 1);
}
- (void)accessibilityDecrement {
    if (self.controlEnabled && self.actionKind == 1 &&
        [MoteAccessibilityView.accessibilityElements containsObject:self])
        mote_ios_accessibility_action(self.controlID, 2);
}
@end

void mote_ios_accessibility_publish(const uint8_t *bytes, size_t length) {
    @autoreleasepool {
        if (!MoteAccessibilityView) return;
        NSData *data = [NSData dataWithBytes:bytes length:length];
        NSArray<NSDictionary *> *items = [NSJSONSerialization JSONObjectWithData:data options:0 error:nil];
        if (![items isKindOfClass:[NSArray class]]) return;
        NSMutableArray<MoteControlElement *> *elements = [NSMutableArray arrayWithCapacity:items.count];
        NSMutableDictionary<NSNumber *, MoteControlElement *> *current = [NSMutableDictionary dictionaryWithCapacity:items.count];
        for (NSDictionary *item in items) {
            if (![item isKindOfClass:[NSDictionary class]]) continue;
            NSString *role = item[@"role"];
            NSString *label = item[@"label"];
            NSArray<NSNumber *> *rect = item[@"rect"];
            if (![role isKindOfClass:[NSString class]] || ![label isKindOfClass:[NSString class]] ||
                ![rect isKindOfClass:[NSArray class]] || rect.count != 4) continue;
            CGRect frame = CGRectMake(rect[0].doubleValue, rect[1].doubleValue,
                                      rect[2].doubleValue, rect[3].doubleValue);
            if (CGRectIsEmpty(frame) || !isfinite(frame.origin.x) || !isfinite(frame.origin.y)) continue;
            NSNumber *identity = item[@"id"];
            if (![identity isKindOfClass:[NSNumber class]] || current[identity]) continue;
            MoteControlElement *element = MoteAccessibilityElements[identity];
            if (![element.controlRole isEqualToString:role]) element = nil;
            if (!element) element = [[MoteControlElement alloc] initWithAccessibilityContainer:MoteAccessibilityView];
            element.controlID = identity.unsignedLongLongValue;
            element.controlRole = role;
            element.controlEnabled = [item[@"enabled"] boolValue];
            element.accessibilityLabel = label;
            element.accessibilityValue = [item[@"value"] isKindOfClass:[NSString class]] ? item[@"value"] : nil;
            element.accessibilityFrameInContainerSpace = frame;
            element.actionKind = 255;
            if ([role isEqualToString:@"button"] || [role isEqualToString:@"toggle"] ||
                [role isEqualToString:@"select"]) {
                element.accessibilityTraits = UIAccessibilityTraitButton;
                element.actionKind = 0;
            } else if ([role isEqualToString:@"slider"]) {
                element.accessibilityTraits = UIAccessibilityTraitAdjustable;
                element.actionKind = 1;
            } else if ([role isEqualToString:@"text_field"]) {
                element.actionKind = 3;
            } else {
                element.accessibilityTraits = UIAccessibilityTraitStaticText;
            }
            if (!element.controlEnabled) element.accessibilityTraits |= UIAccessibilityTraitNotEnabled;
            [elements addObject:element];
            current[identity] = element;
        }
        MoteAccessibilityElements = current;
        MotePublishedAccessibilityElements = elements;
        MoteAccessibilityView.isAccessibilityElement = NO;
        refreshMoteAccessibilityElements();
    }
}

static void attachWindowToScene(void) {
    if (MoteWindow == nil || MoteWindowScene == nil) {
        return;
    }
    MoteWindow.windowScene = MoteWindowScene;
    MoteWindow.frame = MoteWindowScene.coordinateSpace.bounds;
    [MoteWindow makeKeyAndVisible];
}

void mote_ios_attach_view(void *view) {
    MoteAccessibilityView = (__bridge UIView *)view;
    if (!MoteTextInput) MoteTextInput = [[MoteTextInputProxy alloc] initWithFrame:CGRectZero];
    if (MoteTextInput.superview != MoteAccessibilityView) {
        [MoteTextInput removeFromSuperview];
        [MoteAccessibilityView addSubview:MoteTextInput];
    }
    MoteTextInput.hidden = YES;
    MoteTextInput.userInteractionEnabled = NO;
    MoteWindow = MoteAccessibilityView.window;
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
        NSArray<NSString *> *arguments = NSProcessInfo.processInfo.arguments;
        NSUInteger pasteboard = [arguments indexOfObject:@"--mote-test-pasteboard"];
        if (pasteboard != NSNotFound && pasteboard + 1 < arguments.count) {
            UIPasteboard.generalPasteboard.string = arguments[pasteboard + 1];
        }
        MoteNativeInputTestEnabled =
            [arguments containsObject:@"--mote-test-native-input-sequence"];
        mote_ios_main();
    }
}
