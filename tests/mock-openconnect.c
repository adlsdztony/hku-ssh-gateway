/* Terminal/lifecycle fixture only; never connects to a VPN server. */
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

static volatile sig_atomic_t stopping;
static void stop(int signal_number) { (void)signal_number; stopping = 1; }

int main(void) {
    if (!isatty(STDIN_FILENO)) return 10;
    if (!getpass("Password: ")) return 11;
    if (!getpass("MFA: ")) return 12;
    if (access("/run/hku-test-auth-fail", F_OK) == 0) return 1;
    signal(SIGINT, stop);
    signal(SIGTERM, stop);
    signal(SIGHUP, stop);
    if (system("ip tuntap add dev tun0 mode tun && ip address add 10.250.254.1/32 dev tun0 && ip link set tun0 up && ip route replace default dev tun0") != 0) return 13;
    puts("Test tunnel configured.");
    fflush(stdout);
    while (!stopping) pause();
    (void)system("ip link delete tun0");
    return 0;
}
