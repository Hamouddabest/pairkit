#import <UIKit/UIKit.h>
#import "Launcher.h"
#import "PairKit-Swift.h"

int main(int argc, char *argv[]) {
    int r = PKRunGuestIfRequested(argc, argv);
    if (r != -1) return r;
    [PKEntry runHost];
    return 0;
}
