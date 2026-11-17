#include "../zygisk/jni/parcel_bounds.hpp"
#include <array>
#include <cassert>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <limits>
int main() {
    alignas(uint32_t) std::array<unsigned char,16> data{};
    uint32_t value=3;
    std::memcpy(data.data(),&value,sizeof(value));
    detach_parcel::Cursor cursor{data.data(),data.size(),0};
    uint32_t got=0;
    assert(cursor.read_u32(got) && got==3);
    const char16_t* word=nullptr;
    assert(cursor.read_utf16(1,word));
    assert(word[0]==0 && word[1]==0);
    const size_t before=cursor.offset;
    assert(!cursor.skip(10000000) && cursor.offset==before);
    assert(!cursor.read_utf16(UINT32_MAX,word) && cursor.offset==before);
    detach_parcel::Cursor missing{nullptr,5,0};
    assert(!missing.read_u32(got));

    alignas(char16_t) std::array<char16_t,2> good{u'A',0};
    const auto* bytes=reinterpret_cast<const unsigned char*>(good.data());
    detach_parcel::Cursor exact{bytes,sizeof(good),0};
    assert(exact.read_utf16(1,word) && word[0]==u'A' && exact.offset==sizeof(good));
    const size_t after=exact.offset;
    assert(!exact.read_utf16(0,word) && exact.offset==after);

    // A missing terminator, truncated input and a huge claimed length are
    // rejected without reading outside the actual buffer or moving the cursor.
    alignas(char16_t) std::array<char16_t,2> bad_term{u'A',u'B'};
    detach_parcel::Cursor unterminated{
        reinterpret_cast<const unsigned char*>(bad_term.data()),sizeof(bad_term),0};
    assert(!unterminated.read_utf16(1,word) && unterminated.offset==0);
    detach_parcel::Cursor truncated{bytes,3,0};
    assert(!truncated.read_utf16(1,word) && truncated.offset==0);
    detach_parcel::Cursor oversized{bytes,sizeof(good),0};
    assert(!oversized.read_utf16(UINT32_MAX,word) && oversized.offset==0);
    detach_parcel::Cursor underflow{bytes,1,0};
    assert(!underflow.read_utf16(0,word) && underflow.offset==0);
    detach_parcel::Cursor invalid_offset{bytes,3,4};
    assert(!invalid_offset.read_utf16(0,word) && invalid_offset.offset==4);
    detach_parcel::Cursor huge_offset{
        bytes,std::numeric_limits<size_t>::max(),std::numeric_limits<size_t>::max()-1};
    assert(!huge_offset.read_utf16(0,word) &&
           huge_offset.offset==std::numeric_limits<size_t>::max()-1);
    detach_parcel::Cursor misaligned{bytes+1,sizeof(good)-1,0};
    assert(!misaligned.read_utf16(0,word) && misaligned.offset==0);

    alignas(char16_t) std::array<char16_t,1> empty{0};
    detach_parcel::Cursor zero{
        reinterpret_cast<const unsigned char*>(empty.data()),sizeof(empty),0};
    assert(zero.read_utf16(0,word) && zero.offset==sizeof(empty));
}
