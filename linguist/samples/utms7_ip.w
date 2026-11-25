sc.false
// Real-world Wand sample. Source: /home/user12ms/Projects/utms7/apps/net/ip.w
extern fn uz_puts(u8* s) -> i32;
extern fn uz_print_ip(u8* name, u32 ip) -> i32;
extern fn uz_print_mac(u8* mac) -> i32;
extern fn net_get_ip() -> u32;
extern fn net_get_netmask() -> u32;
extern fn net_get_gateway() -> u32;
extern fn net_get_dns() -> u32;
extern fn net_get_mac() -> u8*;
extern fn net_is_ready() -> i32;
extern fn strcmp(u8* a, u8* b) -> i32;

export fn ip_main(i32 argc, u64 argv_ptr) -> i32 {
    u64* argv = (u64*)argv_ptr;
    if (argc >= 2 && strcmp((u8*)argv[1], "route") == 0) {
        uz_print_ip("default via ", net_get_gateway());
        return(0);
    }
    if (net_is_ready() == 0) { uz_puts("net: not ready in this process\n"); }
    uz_print_mac(net_get_mac());
    uz_print_ip("  ip=", net_get_ip());
    uz_print_ip("  mask=", net_get_netmask());
    uz_print_ip("  gw=", net_get_gateway());
    uz_print_ip("  dns=", net_get_dns());
    return(0);
}
