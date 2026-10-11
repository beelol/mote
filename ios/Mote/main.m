#import "mote_ios.h"
#import <UIKit/UIKit.h>
#import <TargetConditionals.h>
#import <Security/Security.h>
#include <dns_sd.h>
#include <arpa/inet.h>
#include <poll.h>

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
        status = DNSServiceBrowse(&browse, kDNSServiceFlagsShareConnection, kDNSServiceInterfaceIndexLocalOnly,
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
