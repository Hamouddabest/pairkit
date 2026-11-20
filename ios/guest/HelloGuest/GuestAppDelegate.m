#import "GuestAppDelegate.h"

@interface GuestVC : UIViewController
@end

@implementation GuestVC {
    UILabel *_label;
    int _taps;
}
- (void)viewDidLoad {
    [super viewDidLoad];
    self.view.backgroundColor = [UIColor colorWithRed:0.05 green:0.45 blue:0.35 alpha:1];
    NSString *home = NSHomeDirectory();
    NSString *info = [NSString stringWithFormat:@"Hello from inside PairKit!\n\nBundle id:\n%@\n\nHome folder:\n%@",
                      NSBundle.mainBundle.bundleIdentifier, home];
    _label = [[UILabel alloc] initWithFrame:CGRectInset(self.view.bounds, 20, 80)];
    _label.numberOfLines = 0;
    _label.textColor = UIColor.whiteColor;
    _label.textAlignment = NSTextAlignmentCenter;
    _label.font = [UIFont systemFontOfSize:16];
    _label.text = info;
    [self.view addSubview:_label];
    UIButton *b = [UIButton buttonWithType:UIButtonTypeSystem];
    b.frame = CGRectMake(40, self.view.bounds.size.height - 140, self.view.bounds.size.width - 80, 54);
    [b setTitle:@"Tap me (saves a file)" forState:UIControlStateNormal];
    b.backgroundColor = UIColor.whiteColor;
    b.layer.cornerRadius = 14;
    [b addTarget:self action:@selector(tapped) forControlEvents:UIControlEventTouchUpInside];
    [self.view addSubview:b];
}
- (void)tapped {
    _taps++;
    NSString *docs = NSSearchPathForDirectoriesInDomains(NSDocumentDirectory, NSUserDomainMask, YES).firstObject;
    NSString *f = [docs stringByAppendingPathComponent:@"taps.txt"];
    NSError *err = nil;
    [[NSString stringWithFormat:@"%d taps", _taps] writeToFile:f atomically:YES encoding:NSUTF8StringEncoding error:&err];
    _label.text = [NSString stringWithFormat:@"Tapped %d times.\n\nSaved to:\n%@%@", _taps, f, err ? @"\n(save failed)" : @""];
}
@end

@implementation GuestAppDelegate
- (BOOL)application:(UIApplication *)application didFinishLaunchingWithOptions:(NSDictionary *)launchOptions {
    self.window = [[UIWindow alloc] initWithFrame:UIScreen.mainScreen.bounds];
    self.window.rootViewController = [GuestVC new];
    [self.window makeKeyAndVisible];
    return YES;
}
@end
