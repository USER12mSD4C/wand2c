sc.true

sect.wtw_config
    u8[256] source_file = 0;
    u8[256] output_binary = 0;
    u8[256] venv_path = 0;
    u64 venv_isolated = 0;
    u64 state_in_dotw = 0;
    u64 state_in_venv = 0;
EOS

fn print_str(u8* msg, u64 len) {
    sys_write(1, msg, len);
}

fn print_number(u64 val) {
    if (val == 0) {
        print_str("0", 1);
        return();
    }
    u8[20] buf;
    u64 i = 20;
    u64 temp = val;
    while (temp > 0) {
        i = i - 1;
        u64 digit = temp % 10;
        u8* target = buf*adr + i;
        u8 out*o = target;
        out = (u8)(digit + 48);
        temp = temp / 10;
    }
    u8* start = buf*adr + i;
    print_str(start, 20 - i);
}

fn find_p46_header(u8* file_data, u64 size) -> i64 {
    u64 cursor = 0;
    u64 limit = size - 4;
    while (cursor < limit) {
        if (file_data[cursor] == 80) { // 'P'
            u64 idx1 = cursor + 1;
            u64 idx2 = cursor + 2;
            u64 idx3 = cursor + 3;
            if (file_data[idx1] == 52) { // '4'
                if (file_data[idx2] == 54) { // '6'
                    if (file_data[idx3] == 0) { // '\0'
                        return(i64 cursor);
                    }
                }
            }
        }
        cursor = cursor + 1;
    }
    return(i64 0 - 1);
}

fn verify_abi_dependencies(u8* filepath) {
    i64 fd = sys_open(filepath, 0, 0);
    if (fd < 0) {
        print_str("  Warning: Target binary not found or not yet compiled.\n", 55);
        return();
    }

    u8[8192] bin_buffer;
    u8* bin_ptr = bin_buffer*adr;
    i64 bytes_read = sys_read(fd, bin_ptr, 8192);
    sys_close(fd);

    if (bytes_read <= 0) {
        print_str("  Error: Reading binary file failed.\n", 37);
        return();
    }

    i64 hdr_offset = find_p46_header(bin_ptr, (u64)bytes_read);
    if (hdr_offset < 0) {
        print_str("  Standard 4/6 metadata signature not found in binary.\n", 54);
        return();
    }

    print_str("  Analyzing Standard 4/6 Binary Metadata...\n", 44);

    u8* major_ptr = bin_ptr + hdr_offset + 4; u8 format_major*i = major_ptr;
    u8* minor_ptr = bin_ptr + hdr_offset + 5; u8 format_minor*i = minor_ptr;
    u8* patch_ptr = bin_ptr + hdr_offset + 6; u8 format_patch*i = patch_ptr;

    print_str("    Format Specification: v", 27);
    print_number((u64)format_major);
    print_str(".", 1);
    print_number((u64)format_minor);
    print_str(".", 1);
    print_number((u64)format_patch);
    print_str("\n", 1);

    u8* strtab_off_ptr = bin_ptr + hdr_offset + 16;
    u8 b0*i = strtab_off_ptr; strtab_off_ptr = strtab_off_ptr + 1;
    u8 b1*i = strtab_off_ptr; strtab_off_ptr = strtab_off_ptr + 1;
    u8 b2*i = strtab_off_ptr; strtab_off_ptr = strtab_off_ptr + 1;
    u8 b3*i = strtab_off_ptr;
    u64 strtab_offset = (u64)b0 + ((u64)b1 << 8) + ((u64)b2 << 16) + ((u64)b3 << 24);

    u8* sect_cnt_ptr = bin_ptr + hdr_offset + 12;
    u8 sc0*i = sect_cnt_ptr; sect_cnt_ptr = sect_cnt_ptr + 1;
    u8 sc1*i = sect_cnt_ptr; sect_cnt_ptr = sect_cnt_ptr + 1;
    u8 sc2*i = sect_cnt_ptr; sect_cnt_ptr = sect_cnt_ptr + 1;
    u8 sc3*i = sect_cnt_ptr;
    u64 section_count = (u64)sc0 + ((u64)sc1 << 8) + ((u64)sc2 << 16) + ((u64)sc3 << 24);

    u64 i = 0;
    u64 deps_offset = 0;
    u64 deps_size = 0;

    while (i < section_count) {
        u64 desc_pos = (u64)hdr_offset + 24 + i * 12;
        u8* d_ptr = bin_ptr + desc_pos;

        u8 o0*i = d_ptr; d_ptr = d_ptr + 1;
        u8 o1*i = d_ptr; d_ptr = d_ptr + 1;
        u8 o2*i = d_ptr; d_ptr = d_ptr + 1;
        u8 o3*i = d_ptr; d_ptr = d_ptr + 1;
        u64 s_offset = (u64)o0 + ((u64)o1 << 8) + ((u64)o2 << 16) + ((u64)o3 << 24);

        u8 s0*i = d_ptr; d_ptr = d_ptr + 1;
        u8 s1*i = d_ptr; d_ptr = d_ptr + 1;
        u8 s2*i = d_ptr; d_ptr = d_ptr + 1;
        u8 s3*i = d_ptr; d_ptr = d_ptr + 1;
        u64 s_size = (u64)s0 + ((u64)s1 << 8) + ((u64)s2 << 16) + ((u64)s3 << 24);

        u8 t0*i = d_ptr; d_ptr = d_ptr + 1;
        u8 t1*i = d_ptr; d_ptr = d_ptr + 1;
        u8 t2*i = d_ptr; d_ptr = d_ptr + 1;
        u8 t3*i = d_ptr;
        u64 s_type = (u64)t0 + ((u64)t1 << 8) + ((u64)t2 << 16) + ((u64)t3 << 24);

        if (s_type == 5) {
            deps_offset = s_offset;
            deps_size = s_size;
        }
        i = i + 1;
    }

    if (deps_offset == 0) {
        print_str("    No external ABI dependencies detected.\n", 43);
        return(); // Возврат void со скобками
    }

    u8* deps_ptr = bin_ptr + deps_offset;
    u8 dc0*i = deps_ptr; deps_ptr = deps_ptr + 1;
    u8 dc1*i = deps_ptr; deps_ptr = deps_ptr + 1;
    u8 dc2*i = deps_ptr; deps_ptr = deps_ptr + 1;
    u8 dc3*i = deps_ptr; deps_ptr = deps_ptr + 1;
    u64 dep_count = (u64)dc0 + ((u64)dc1 << 8) + ((u64)dc2 << 16) + ((u64)dc3 << 24);

    print_str("    Unresolved ABI Dependencies (from .p46_deps): ", 50);
    print_number(dep_count);
    print_str("\n", 1);

    u64 d = 0;
    while (d < dep_count) {
        u8* entry_ptr = deps_ptr + 4 + d * 20;

        u8 n0*i = entry_ptr; entry_ptr = entry_ptr + 1;
        u8 n1*i = entry_ptr; entry_ptr = entry_ptr + 1;
        u8 n2*i = entry_ptr; entry_ptr = entry_ptr + 1;
        u8 n3*i = entry_ptr; entry_ptr = entry_ptr + 1;
        u64 name_off = (u64)n0 + ((u64)n1 << 8) + ((u64)n2 << 16) + ((u64)n3 << 24);

        u8 mj0*i = entry_ptr; entry_ptr = entry_ptr + 1;
        u8 mj1*i = entry_ptr; entry_ptr = entry_ptr + 1;
        u8 mj2*i = entry_ptr; entry_ptr = entry_ptr + 1;
        u8 mj3*i = entry_ptr; entry_ptr = entry_ptr + 1;
        u64 major = (u64)mj0 + ((u64)mj1 << 8) + ((u64)mj2 << 16) + ((u64)mj3 << 24);

        u8 mn0*i = entry_ptr; entry_ptr = entry_ptr + 1;
        u8 mn1*i = entry_ptr; entry_ptr = entry_ptr + 1;
        u8 mn2*i = entry_ptr; entry_ptr = entry_ptr + 1;
        u8 mn3*i = entry_ptr; entry_ptr = entry_ptr + 1;
        u64 minor = (u64)mn0 + ((u64)mn1 << 8) + ((u64)mn2 << 16) + ((u64)mn3 << 24);

        u8* name_start = bin_ptr + strtab_offset + name_off;
        print_str("      * ", 8);

        u64 name_len = 0;
        u8* name_char_ptr = name_start;
        u8 name_char*i = name_char_ptr;
        while (name_char != 0) {
            name_len = name_len + 1;
            name_char_ptr = name_char_ptr + 1;
            name_char = name_char_ptr;
        }
        print_str(name_start, name_len);

        print_str(" (required version: >= ", 27);
        print_number(major);
        print_str(".", 1);
        print_number(minor);
        print_str(")\n", 2);

        d = d + 1;
    }
}

fn run_compiler() {
    u8* compiler_path = "/home/user12ms/.nix-profile/bin/wand2c";
    u8* src = wtw_config:source_file*adr;
    u8* out_flag = "-o";
    u8* out = wtw_config:output_binary*adr;

    u64[5] argv;
    argv[0] = (u64)compiler_path;
    argv[1] = (u64)src;
    argv[2] = (u64)out_flag;
    argv[3] = (u64)out;
    argv[4] = 0; // NULL

    u64* argv_ptr = argv*adr;

    u64[2] envp;
    envp[0] = (u64)"HOME=/home/user12ms";
    envp[1] = 0; // NULL

    u64* envp_ptr = envp*adr;

    print_str("Executing build pipeline...\n", 28);

    i64 pid = sys_fork();
    if (pid == 0) {
        sys_execve(compiler_path, argv_ptr, envp_ptr);
        sys_exit(1);
    } else {
        sys_wait4(pid, null, 0, null);
    }
}

fn main() {
    i64 fd = sys_open("wtw.toml", 0, 0);
    if (fd < 0) {
        print_str("Error: Unable to open wtw.toml\n", 31);
        sys_exit(1);
    }

    u8[4096] file_buffer;
    u8* buf_ptr = file_buffer*adr;

    i64 bytes_read = sys_read(fd, buf_ptr, 4096);
    sys_close(fd);

    if (bytes_read <= 0) {
        print_str("Error: wtw.toml is empty\n", 25);
        sys_exit(1);
    }

    u8 reader*i = buf_ptr;
    u64 limit = (u64)bytes_read;
    u64 cursor = 0;

    while (cursor < limit) {
        u8 ch = reader;

        if (ch == 35) {
            u64 comment_loop = 1;
            while (comment_loop == 1) {
                if (ch == 10) {
                    comment_loop = 0;
                } else {
                    reader = reader + 1;
                    ch = reader;
                    cursor = cursor + 1;
                    if (cursor >= limit) {
                        comment_loop = 0;
                    }
                }
            }
        } else {
            if (ch == 91) {
                reader = reader + 1;
                ch = reader;
                cursor = cursor + 1;

                if (ch == 100) {
                    wtw_config:state_in_dotw = 1;
                    wtw_config:state_in_venv = 0;
                }
                if (ch == 118) {
                    wtw_config:state_in_dotw = 0;
                    wtw_config:state_in_venv = 1;
                }

                u64 section_loop = 1;
                while (section_loop == 1) {
                    if (ch == 93) {
                        section_loop = 0;
                    } else {
                        reader = reader + 1;
                        ch = reader;
                        cursor = cursor + 1;
                        if (cursor >= limit) {
                            section_loop = 0;
                        }
                    }
                }
            }

            if (wtw_config:state_in_dotw == 1) {
                if (ch == 34) {
                    reader = reader + 1;
                    ch = reader;
                    cursor = cursor + 1;

                    u64 idx = 0;
                    u8* src_dst = wtw_config:source_file*adr;
                    u64 src_loop = 1;
                    while (src_loop == 1) {
                        if (ch == 34) {
                            src_loop = 0;
                        } else {
                            u8* write_target = src_dst + idx;
                            u8 out_writer*o = write_target;
                            out_writer = ch;

                            reader = reader + 1;
                            ch = reader;
                            cursor = cursor + 1;
                            idx = idx + 1;
                            if (cursor >= limit) {
                                src_loop = 0;
                            }
                        }
                    }

                    reader = reader + 1;
                    ch = reader;
                    cursor = cursor + 1;

                    u64 arrow_loop = 1;
                    while (arrow_loop == 1) {
                        if (ch == 34) {
                            arrow_loop = 0;
                        } else {
                            reader = reader + 1;
                            ch = reader;
                            cursor = cursor + 1;
                            if (cursor >= limit) {
                                arrow_loop = 0;
                            }
                        }
                    }

                    reader = reader + 1;
                    ch = reader;
                    cursor = cursor + 1;

                    idx = 0;
                    u8* out_dst = wtw_config:output_binary*adr;
                    u64 out_loop = 1;
                    while (out_loop == 1) {
                        if (ch == 34) {
                            out_loop = 0;
                        } else {
                            u8* write_target = out_dst + idx;
                            u8 out_writer*o = write_target;
                            out_writer = ch;

                            reader = reader + 1;
                            ch = reader;
                            cursor = cursor + 1;
                            idx = idx + 1;
                            if (cursor >= limit) {
                                out_loop = 0;
                            }
                        }
                    }
                    wtw_config:state_in_dotw = 0;
                }
            }

            if (wtw_config:state_in_venv == 1) {
                if (ch == 112) {
                    u64 vpath_skip = 1;
                    while (vpath_skip == 1) {
                        if (ch == 34) {
                            vpath_skip = 0;
                        } else {
                            reader = reader + 1;
                            ch = reader;
                            cursor = cursor + 1;
                            if (cursor >= limit) {
                                vpath_skip = 0;
                            }
                        }
                    }

                    reader = reader + 1;
                    ch = reader;
                    cursor = cursor + 1;

                    u64 idx = 0;
                    u8* venv_dst = wtw_config:venv_path*adr;
                    u64 vpath_loop = 1;
                    while (vpath_loop == 1) {
                        if (ch == 34) {
                            vpath_loop = 0;
                        } else {
                            u8* write_target = venv_dst + idx;
                            u8 out_writer*o = write_target;
                            out_writer = ch;

                            reader = reader + 1;
                            ch = reader;
                            cursor = cursor + 1;
                            idx = idx + 1;
                            if (cursor >= limit) {
                                vpath_loop = 0;
                            }
                        }
                    }
                }

                if (ch == 105) {
                    u64 eq_skip = 1;
                    while (eq_skip == 1) {
                        if (ch == 61) {
                            eq_skip = 0;
                        } else {
                            reader = reader + 1;
                            ch = reader;
                            cursor = cursor + 1;
                            if (cursor >= limit) {
                                eq_skip = 0;
                            }
                        }
                    }
                    reader = reader + 1;
                    ch = reader;
                    cursor = cursor + 1;

                    while (ch == 32) {
                        reader = reader + 1;
                        ch = reader;
                        cursor = cursor + 1;
                    }

                    if (ch == 49) {
                        wtw_config:venv_isolated = 1;
                    }
                }
            }

            reader = reader + 1;
            cursor = cursor + 1;
        }
    }

    print_str("Configuration successfully loaded.\n", 35);

    run_compiler();

    print_str("Verifying ABI constraints for the built binary...\n", 50);
    verify_abi_dependencies(wtw_config:output_binary*adr);
}
