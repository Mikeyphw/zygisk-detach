#pragma once
#include <cstddef>
#include <cstdint>
#include <cstring>
namespace detach_parcel {
struct Cursor {
    const unsigned char* data;
    size_t length;
    size_t offset;
    bool skip(size_t amount) {
        if (!data || offset > length || amount > length - offset) return false;
        offset += amount; return true;
    }
    bool read_u32(uint32_t& value) {
        if (!data || offset > length || length-offset < sizeof(value)) return false;
        std::memcpy(&value, data+offset, sizeof(value));
        offset += sizeof(value); return true;
    }
    bool read_utf16(uint32_t code_units, const char16_t*& result) {
        // Compare the requested element count against *remaining input* before
        // multiplying. This works on 32/64-bit size_t without a tautological
        // 64-bit SIZE_MAX versus uint32_t comparison under Clang -Werror.
        if (!data || offset > length) return false;
        const size_t remaining = length - offset;
        if (remaining < sizeof(char16_t)) return false;
        const size_t units = static_cast<size_t>(code_units);
        if (units > remaining / sizeof(char16_t) - 1) return false;
        const size_t amount = (units + 1) * sizeof(char16_t);
        const auto* address = data + offset;
        if (reinterpret_cast<uintptr_t>(address) % alignof(char16_t)) return false;
        const auto* candidate = reinterpret_cast<const char16_t*>(address);
        if (candidate[units] != 0) return false;
        result = candidate;
        offset += amount;
        return true;
    }
};
} // namespace detach_parcel
