sc.true

#import <syscall>

sect.rand_state
    u64 next = 1;
EOS

export fn exit(u64 code) {
    sys_exit(code);
}

export fn srand(u64 seed) {
    rand_state:next = seed;
}

export fn rand() -> u64 {
    u64 current = rand_state:next;
    u64 multiplier = 6364136223846793005;
    u64 increment = 1442695040888963407;
    u64 next_val = (current * multiplier) + increment;
    rand_state:next = next_val;
    return(next_val);
}
