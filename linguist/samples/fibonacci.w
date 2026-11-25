sc.true
// Real-world Wand sample. Source: /home/user12ms/experiment/tasks/fibonacci/fibonacci.w
#import <io>
#import <args>
#import <string>

// Recursive Fibonacci: function-call and branch overhead.
// A global call counter prevents the optimizer from replacing the
// recurrence with a closed form. depth = 26 + (n % 3); n comes from argv
// so the depth is not a compile-time constant.
// Checksum = folded results XOR total call count.
sect.state
    u64 calls = 0;
EOS

fn fib(u64 x) -> u64 {
    state:calls = state:calls + 1;
    if (x < 2) { return(x); }
    return(fib(x - 1) + fib(x - 2));
}

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    u64 n = atoi(get_arg(argv, 1));
    u64 depth = 26 + (n % 3);
    u64 acc = 0;
    for (u64 k = 0; k < n; k++) {
        acc = acc * 3 + fib(depth);
    }
    printf("%v\n", acc ^ state:calls, 0, 0);
    return(0);
}
