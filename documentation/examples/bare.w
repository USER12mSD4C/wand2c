sc.false

sect.kernel
    u64 ticks = 0;
EOS

fn kmain() {
    critical {
        kernel:ticks = kernel:ticks + 1;
    }
    ::nasm::{
        hlt
    }
}
