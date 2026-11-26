CC ?= cc
AR ?= ar
CFLAGS ?= -O2 -fPIC -std=c11 -Wall -Wextra -Werror
CPPFLAGS ?= -I../include

SRC := ../src/loader/p46_loader.c
HDR := ../include/wand_p46.h

all: libwand_p46.a libwand_p46.so

libwand_p46.a: $(SRC) $(HDR)
	$(CC) $(CPPFLAGS) $(CFLAGS) -c $(SRC) -o p46_loader.o
	$(AR) rcs $@ p46_loader.o

libwand_p46.so: $(SRC) $(HDR)
	$(CC) $(CPPFLAGS) $(CFLAGS) -shared $(SRC) -o $@

clean:
	rm -f p46_loader.o libwand_p46.a libwand_p46.so

.PHONY: all clean
