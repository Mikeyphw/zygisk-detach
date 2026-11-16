#include "../zygisk/jni/legacy_codec.hpp"
#include <cassert>
#include <string>
#include <vector>
int main() {
    std::vector<char> v;
    auto add=[](std::vector<char>& b,const std::string& n){
        b.push_back(static_cast<char>(n.size()*2-1));
        for (size_t i=0;i<n.size();++i) {b.push_back(n[i]);if(i+1<n.size())b.push_back(0);}
    };
    add(v,"com.google.android.youtube");
    add(v,"org.example_app.demo");
    assert(detach_codec::validate(v.data(),v.size()));
    assert(!detach_codec::validate(v.data(),v.size()-1));
    assert(!detach_codec::validate(nullptr,1));
    auto bad=v;bad[2]='x';assert(!detach_codec::validate(bad.data(),bad.size()));
    bad=v;bad[0]=0;assert(!detach_codec::validate(bad.data(),bad.size()));
    assert(!detach_codec::validate("",1024*1024+1));
}
