sc.true
#import <io>
#import "numio"

sect.state
    u64 ticks = 0;
EOS

fn bump() {
    state:ticks = state:ticks + 1;
}

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    bump();
    bump();
    show(state:ticks);
    print_char(10);
    return(0);
}
