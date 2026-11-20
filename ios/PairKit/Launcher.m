// Runs a "guest" app inside PairKit's process (a minimal container).
// PairKit writes Documents/.pairkit/launch.txt (path of the guest's folder) and restarts;
// main() sees that file before any UI starts and loads the guest instead.

#import <Foundation/Foundation.h>
#import <UIKit/UIKit.h>
#import <mach-o/loader.h>
#import <mach-o/dyld.h>
#import <dlfcn.h>
#import <objc/runtime.h>
#import "Launcher.h"

static NSBundle *gGuestBundle = nil;

@implementation NSBundle (PKGuest)
+ (NSBundle *)pk_mainBundle {
    return gGuestBundle ? gGuestBundle : [self pk_mainBundle]; // swizzled: calls the original when no guest
}
@end

static NSString *PKDocs(void) {
    return NSSearchPathForDirectoriesInDomains(NSDocumentDirectory, NSUserDomainMask, YES).firstObject;
}

static void PKLog(NSString *line) {
    NSString *path = [PKDocs() stringByAppendingPathComponent:@".pairkit/launch_log.txt"];
    NSString *old = [NSString stringWithContentsOfFile:path encoding:NSUTF8StringEncoding error:nil] ?: @"";
    NSString *neu = [old stringByAppendingFormat:@"%@\n", line];
    [neu writeToFile:path atomically:YES encoding:NSUTF8StringEncoding error:nil];
}

int PKRunGuestIfRequested(int argc, char *argv[]) {
    @autoreleasepool {
        NSFileManager *fm = NSFileManager.defaultManager;
        NSString *flag = [PKDocs() stringByAppendingPathComponent:@".pairkit/launch.txt"];
        NSString *dir = [[NSString stringWithContentsOfFile:flag encoding:NSUTF8StringEncoding error:nil]
                         stringByTrimmingCharactersInSet:NSCharacterSet.whitespaceAndNewlineCharacterSet];
        if (dir.length == 0) return -1;
        [fm removeItemAtPath:flag error:nil];
        [fm removeItemAtPath:[PKDocs() stringByAppendingPathComponent:@".pairkit/launch_log.txt"] error:nil];

        NSString *appPath = [dir stringByAppendingPathComponent:@"App.app"];
        NSString *dataPath = [dir stringByAppendingPathComponent:@"Data"];
        for (NSString *sub in @[@"Documents", @"Library/Caches", @"Library/Preferences", @"tmp"]) {
            [fm createDirectoryAtPath:[dataPath stringByAppendingPathComponent:sub] withIntermediateDirectories:YES attributes:nil error:nil];
        }
        NSBundle *guest = [NSBundle bundleWithPath:appPath];
        NSString *exe = guest.infoDictionary[@"CFBundleExecutable"];
        if (!guest || exe.length == 0) {
            PKLog([NSString stringWithFormat:@"FAILED: no app at %@", appPath]);
            return -1;
        }
        NSString *exePath = [appPath stringByAppendingPathComponent:exe];
        PKLog([NSString stringWithFormat:@"Loading %@", exe]);

        // Send the guest's files to its own folder.
        setenv("CFFIXED_USER_HOME", dataPath.UTF8String, 1);
        setenv("HOME", dataPath.UTF8String, 1);
        setenv("TMPDIR", [dataPath stringByAppendingPathComponent:@"tmp"].UTF8String, 1);

        // Make NSBundle.mainBundle the guest's bundle.
        gGuestBundle = guest;
        Method a = class_getClassMethod(NSBundle.class, @selector(mainBundle));
        Method b = class_getClassMethod(NSBundle.class, @selector(pk_mainBundle));
        if (a && b) method_exchangeImplementations(a, b);

        void *handle = dlopen(exePath.UTF8String, RTLD_NOW | RTLD_GLOBAL);
        if (!handle) {
            const char *err = dlerror();
            PKLog([NSString stringWithFormat:@"FAILED dlopen: %s", err ? err : "unknown"]);
            gGuestBundle = nil;
            if (a && b) method_exchangeImplementations(a, b);
            return -1;
        }
        PKLog(@"dlopen ok");

        typedef int (*MainFn)(int, char **);
        MainFn entry = (MainFn)dlsym(handle, "main");
        if (!entry) {
            for (uint32_t i = 0; i < _dyld_image_count(); i++) {
                const char *name = _dyld_get_image_name(i);
                if (!name || !strstr(name, "/App.app/")) continue;
                const struct mach_header_64 *mh = (const struct mach_header_64 *)_dyld_get_image_header(i);
                if (!mh || mh->magic != MH_MAGIC_64) continue;
                const uint8_t *p = (const uint8_t *)(mh + 1);
                for (uint32_t c = 0; c < mh->ncmds; c++) {
                    const struct load_command *lc = (const struct load_command *)p;
                    if (lc->cmd == LC_MAIN) {
                        const struct entry_point_command *ep = (const struct entry_point_command *)lc;
                        entry = (MainFn)((const uint8_t *)mh + ep->entryoff);
                    }
                    p += lc->cmdsize;
                }
                if (entry) break;
            }
        }
        if (!entry) {
            PKLog(@"FAILED: could not find the app's main()");
            return -1;
        }
        PKLog(@"calling main()");
        char *gargv[] = { (char *)exePath.UTF8String, NULL };
        return entry(1, gargv);
    }
}
