/* Terminal/lifecycle fixture only; never connects to a VPN server. */
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static volatile sig_atomic_t stopping;
static void stop(int signal_number) { (void)signal_number; stopping = 1; }

int main(int argc, char **argv) {
    if (argc > 1 && strcmp(argv[1], "--version") == 0) { puts("OpenConnect terminal fixture"); return 0; }
    const char *native_hook = NULL;
    for (int i = 1; i < argc; ++i) {
        if (strncmp(argv[i], "--script=", 9) == 0 && strstr(argv[i], "native-internal hook")) native_hook = argv[i] + 9;
    }
    if (!isatty(STDIN_FILENO)) return 10;
    if (!getpass("Password: ")) return 11;
    if (!getpass("MFA: ")) return 12;
    if (access("/run/hku-test-auth-fail", F_OK) == 0) return 1;
    signal(SIGINT, stop);
    signal(SIGTERM, stop);
    signal(SIGHUP, stop);
    if (system("(ip link show tun0 >/dev/null 2>&1 || ip tuntap add dev tun0 mode tun) && ip address add 10.250.254.1/32 dev tun0 && ip link set tun0 up") != 0) return 13;
    if (native_hook) {
        setenv("reason", "connect", 1);
        setenv("TUNDEV", "tun0", 1);
        setenv("VPNGATEWAY", "192.0.2.1", 1);
        setenv("INTERNAL_IP4_ADDRESS", "10.250.254.1", 1);
        setenv("INTERNAL_IP4_NETMASK", "255.255.255.255", 1);
        setenv("INTERNAL_IP4_MTU", "1300", 1);
        if (system(native_hook) != 0) return 15;
    } else if (system("ip route replace default dev tun0") != 0) return 13;
    puts("Test tunnel configured.");
    fflush(stdout);
    while (!stopping) pause();
    if (native_hook) {
        setenv("reason", "disconnect", 1);
        if (system(native_hook) != 0) return 16;
    }
    int cleanup_result = system("ip link delete tun0");
    return cleanup_result == 0 ? 0 : 14;
}
