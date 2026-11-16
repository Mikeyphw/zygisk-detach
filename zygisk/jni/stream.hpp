#pragma once
#include <cerrno>
#include <cstddef>
#include <cstdint>
#include <unistd.h>

namespace detach_io {
inline bool read_full(int fd, void* buf, size_t count) {
    if (fd < 0 || (count && !buf)) return false;
    auto* at = static_cast<unsigned char*>(buf);
    while (count) {
        const ssize_t got = ::read(fd, at, count);
        if (got == 0) return false;  // EOF is not success; never spin forever.
        if (got < 0) { if (errno == EINTR) continue; return false; }
        at += got;
        count -= static_cast<size_t>(got);
    }
    return true;
}
inline bool write_full(int fd, const void* buf, size_t count) {
    if (fd < 0 || (count && !buf)) return false;
    const auto* at = static_cast<const unsigned char*>(buf);
    while (count) {
        const ssize_t sent = ::write(fd, at, count);
        if (sent == 0) return false;
        if (sent < 0) { if (errno == EINTR) continue; return false; }
        at += sent;
        count -= static_cast<size_t>(sent);
    }
    return true;
}
}  // namespace detach_io
