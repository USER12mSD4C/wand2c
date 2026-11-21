sc.true
#import <io>
#import "numio"

fn double(u64 x) -> u64 {
    return(x * 2);
}

fn divmod(u64 a, u64 b, u64* q*o, u64* r*o) {
    q = a / b;
    r = a % b;
}

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    show(double(21));
    print_char(10);

    u64 q;
    u64 r;
    divmod(23, 5, q*adr, r*adr);
    print_string("q=");
    show(q);
    print_string(" r=");
    show(r);
    print_char(10);

    return(0);
}
