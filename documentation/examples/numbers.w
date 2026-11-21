sc.true
#import <io>
#import "numio"

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    u64 apples = 3;
    u64 bananas = 2;
    u64 fruit = apples + bananas;

    print_string("fruit = ");
    show(fruit);
    print_char(10);

    fruit += 5;
    print_string("after += 5: ");
    show(fruit);
    print_char(10);

    return(0);
}
