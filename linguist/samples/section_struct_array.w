sc.true
// Real-world Wand sample. Source: /home/user12ms/experiment/probes/section_struct_array.w
#import <io>

// Correctness probe: section (global) arrays of structs.
// Expected: 4 8. This used to execute a mis-encoded imul instruction.
struct Node version 1 {
    u64 a version 1;
    u64 b version 1;
}

sect.data
    Node nodes[4];
EOS

fn main(u64 argc, u64 argv, u64 envp) -> u64 {
    for (u64 i = 0; i < 4; i++) {
        nodes[i].a = i + 1;
        nodes[i].b = (i + 1) * 2;
    }
    printf("%v %v\n", nodes[3].a, nodes[3].b, 0);
    return(0);
}
