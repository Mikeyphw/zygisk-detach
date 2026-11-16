#include "../zygisk/jni/stream.hpp"
#include <cassert>
#include <csignal>
#include <cstring>
#include <unistd.h>
int main() {
    int p[2]; assert(pipe(p) == 0);
    const char payload[] = "ab";
    assert(write(p[1], payload, 2) == 2);
    close(p[1]);
    char buf[3]{};
    assert(!detach_io::read_full(p[0], buf, sizeof(buf))); // EOF after short read
    close(p[0]);
    assert(pipe(p) == 0);
    assert(detach_io::write_full(p[1], payload, 2));
    char full[2]{};
    assert(detach_io::read_full(p[0], full, 2));
    assert(memcmp(full, payload, 2) == 0);
    close(p[0]); close(p[1]);
    assert(!detach_io::read_full(-1, buf, 1));
    assert(!detach_io::write_full(-1, payload, 1));
}
