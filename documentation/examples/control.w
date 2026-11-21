sc.true
#import <io>
#import "numio"

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    u64 n = 3;

    if (n == 3) {
        print_string("three\n");
    } else {
        print_string("not three\n");
    }

    u64 i = 0;
    while (i < 3) {
        show(i);
        print_char(32);
        i++;
    }
    print_char(10);

    for (u64 j = 0; j < 3; j++) {
        show(j);
        print_char(32);
    }
    print_char(10);

    match (n) {
        case 3 {
            print_string("match: three\n");
        }
        default {
            print_string("match: other\n");
        }
    }

    return(0);
}
