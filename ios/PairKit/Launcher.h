#ifndef LAUNCHER_H
#define LAUNCHER_H
/// Returns -1 when no guest app was requested (start PairKit normally).
int PKRunGuestIfRequested(int argc, char *argv[]);
#endif
