sc.true
#import <io>

fn fill(u8* data*o, u64 size) {
    for (u64 i = 0; i < size; i++) {
        data[i] = 65 + i;
    }
}

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    u8 letters[5];
    fill(letters*adr, 5);

    for (u64 i = 0; i < 5; i++) {
        print_char(letters[i]);
    }
    print_char(10);

    return(0);
}
