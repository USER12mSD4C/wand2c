sc.true

sect.math_const
    i64 PI = 3141592;
    i64 TWO_PI = 6283185;
    i64 HALF_PI = 1570796;
    i64 FIXED_ONE = 1000000;
EOS

fn abs(i64 x) -> i64 {
    if (x < 0) {
        return(0 - x);
    }
    return(x);
}

fn pow(i64 base, u64 exp) -> i64 {
    i64 res = 1;
    for (u64 i = 0; i < exp; i = i + 1) {
        res = res * base;
    }
    return(res);
}

fn sqrt(i64 x) -> i64 {
    if (x <= 0) {
        return(0);
    }
    // Масштабируем число для извлечения корня с точностью до 10^3
    i64 val = x * 1000000;
    i64 res = val / 2;
    if (res == 0) {
        res = 1;
    }

    // Итерационная формула Ньютона (вавилонский метод)
    u64 i = 0;
    while (i < 40) {
        i64 next_res = (res + (val / res)) / 2;
        if (next_res == res) {
            i = 40;
        }
        res = next_res;
        i = i + 1;
    }
    return(res);
}

fn sin(i64 rad) -> i64 {
    // 1. Нормализация угла в диапазон [0, 2*PI]
    i64 x = rad % math_const:TWO_PI;
    if (x < 0) {
        x = x + math_const:TWO_PI;
    }

    // 2. Определение знака квадранта
    i64 sign = 1;
    if (x > math_const:PI) {
        x = x - math_const:PI;
        sign = 0 - 1;
    }

    // 3. Формула Бхаскары I для sin(x) на интервале [0, PI]:
    // sin(x) ~= 16 * x * (PI - x) / (5 * PI^2 - 4 * x * (PI - x))
    i64 pi_minus_x = math_const:PI - x;
    i64 x_prod = x * pi_minus_x;

    i64 num = 16 * x_prod;
    i64 den = 5 * math_const:PI * math_const:PI - 4 * x_prod;

    i64 val = (num * math_const:FIXED_ONE) / den;
    return(val * sign);
}

fn cos(i64 rad) -> i64 {
    // cos(x) = sin(x + PI/2)
    return(sin(rad + math_const:HALF_PI));
}

fn tan(i64 rad) -> i64 {
    i64 s = sin(rad);
    i64 c = cos(rad);
    if (c == 0) {
        // Защита от деления на ноль (возвращаем условную бесконечность)
        return(999999999);
    }
    return((s * math_const:FIXED_ONE) / c);
}

fn print_fixed(i64 x) {
    if (x < 0) {
        print_char(45); // '-'
        x = 0 - x;
    }
    i64 integer_part = x / math_const:FIXED_ONE;
    i64 fractional_part = x % math_const:FIXED_ONE;

    print_number(integer_part);
    print_char(46); // '.'

    // Форматирование ведущих нулей для дробной части
    i64 temp = fractional_part;
    i64 padding_zeros = 0;
    if (temp == 0) {
        padding_zeros = 5;
    } else {
        if (temp < 10) { padding_zeros = 5; }
        else {
            if (temp < 100) { padding_zeros = 4; }
            else {
                if (temp < 1000) { padding_zeros = 3; }
                else {
                    if (temp < 10000) { padding_zeros = 2; }
                    else {
                        if (temp < 100000) { padding_zeros = 1; }
                    }
                }
            }
        }
    }

    for (u64 i = 0; i < padding_zeros; i = i + 1) {
        print_char(48); // '0'
    }
    if (fractional_part > 0) {
        print_number(fractional_part);
    }
}
