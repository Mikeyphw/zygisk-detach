#pragma once
#include <cstddef>
#include <cstdint>
namespace detach_codec {
inline bool is_name_byte(unsigned char c) {
    return (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') ||
           (c >= '0' && c <= '9') || c == '_' || c == '.';
}
inline bool validate(const char* data, std::size_t size) {
    if ((!data && size) || size > 1024 * 1024) return false;
    std::size_t pos = 0;
    while (pos < size) {
        const std::size_t count = static_cast<unsigned char>(data[pos++]);
        if (!count || !(count & 1) || count > size - pos) return false;
        std::size_t dots = 0;
        const std::size_t name_len = (count + 1) / 2;
        if (name_len > 127) return false;
        for (std::size_t i = 0; i < count; ++i) {
            const unsigned char byte = static_cast<unsigned char>(data[pos + i]);
            if (i & 1) { if (byte) return false; }
            else {
                if (!is_name_byte(byte)) return false;
                if (byte == '.') {
                    const std::size_t j = i / 2;
                    if (j == 0 || j + 1 == name_len) return false;
                    if (j && data[pos + i - 2] == '.') return false;
                    ++dots;
                }
            }
        }
        if (!dots) return false;
        pos += count;
    }
    return true;
}
} // namespace detach_codec
