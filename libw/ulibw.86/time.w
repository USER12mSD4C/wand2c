sc.true

#import <syscall>
#import <time>

// UTMS7 time is tick-based. sys_gettime_ticks() wraps kernel get_ticks().
// There is no Linux-correct clock_gettime(timespec) ABI in UTMS7 at this time.

export fn time_ticks() -> u64 {
    return(sys_gettime_ticks());
}

export fn time_sleep_ms(u64 ms) -> u64 {
    return(u_sys_sleep(ms));
}
