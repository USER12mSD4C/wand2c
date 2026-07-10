sc.true

#import <io>
#import <mem>
#import <string>

fn main() {
    // Проверяем чистый системный вызов mloc
    void* ptr = mloc(null, 4096);

    print_string("mloc returned address: ");
    print_number(ptr);
    print_string("\n");

    return(0);
}
