sc.true
#import <io>
#import "numio"

enum Color version 1 {
    Red = 0 version 1;
    Green = 1 version 1;
    Blue = 2 version 1;
}

struct Pixel version 1 {
    u8 r version 1;
    u8 g version 1;
    u8 b version 1;
}

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    Pixel p;
    p.r = 255;
    p.g = 128;
    p.b = 0;

    show(p.r);
    print_char(44);
    show(p.g);
    print_char(44);
    show(p.b);
    print_char(10);

    Color c = Color:Green;
    if (c == Color:Green) {
        print_string("green\n");
    }

    return(0);
}
