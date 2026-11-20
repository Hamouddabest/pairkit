import Foundation

@objc(PKEntry)
final class PKEntry: NSObject {
    @objc static func runHost() {
        PairKitApp.main()
    }
}
