#include "module.hpp"

#include <android/log.h>
#include <errno.h>
#include <fcntl.h>
#include <stdlib.h>
#include <string.h>
#include <sys/sendfile.h>
#include <sys/stat.h>
#include <sys/sysmacros.h>
#include <sys/system_properties.h>
#include <unistd.h>

#include "binder.hpp"
#include "legacy_codec.hpp"
#include "parcel_bounds.hpp"
#include <atomic>
#include "zygisk.hpp"

#define ARR_LEN(a) (sizeof(a) / sizeof((a)[0]))
#define STR_LEN(a) (ARR_LEN(a) - 1)
#define VENDING_PROC "com.android.vending"

#define PM_DESC u"android.content.pm.IPackageManager"

static char* DETACH_TXT = nullptr;
static size_t DETACH_LEN = 0;
static size_t HEADERS_LEN = 0;
static uint32_t getApplicationEnabledSetting_code = 0;

static inline void detach(PParcel* pparcel, uint32_t code) {
    if (!pparcel || !pparcel->data || !DETACH_TXT || !DETACH_LEN ||
        code != getApplicationEnabledSetting_code) return;
    if (pparcel->data_size > 1024 * 1024) return;
    detach_parcel::Cursor cursor{
        reinterpret_cast<const unsigned char*>(pparcel->data),pparcel->data_size,0};
    if (!cursor.skip(HEADERS_LEN)) return;
    uint32_t desc_len=0;
    if (!cursor.read_u32(desc_len) || desc_len!=STR_LEN(PM_DESC)) return;
    const char16_t* desc=nullptr;
    if (!cursor.read_utf16(desc_len,desc)) return;
    if (memcmp(desc,PM_DESC,desc_len*sizeof(char16_t))!=0) return;
    if (!cursor.skip(2)) return; // original Parcel String16 alignment field
    uint32_t pkg_len=0;
    if (!cursor.read_u32(pkg_len) || pkg_len==0 || pkg_len>127) return;
    const char16_t* const_name=nullptr;
    if (!cursor.read_utf16(pkg_len,const_name)) return;
    const size_t encoded_len=static_cast<size_t>(pkg_len)*2-1;
    size_t index=0;
    while (index < DETACH_LEN) {
        const size_t len=static_cast<unsigned char>(DETACH_TXT[index++]);
        if (!len || len>DETACH_LEN-index) return;
        const char* entry=DETACH_TXT+index;
        index+=len;
        if (len!=encoded_len || memcmp(entry,const_name,len)!=0) continue;
        // Only record real matching Binder interceptions. No package names logged.
        static std::atomic<bool> reported{false};
        if (!reported.exchange(true)) LOGD("HOOK_OBSERVED");
        const_cast<char16_t*>(const_name)[0]=0;
        return;
    }
}

int (*transact_orig)(void*, int32_t, uint32_t, void*, void*, uint32_t);

int transact_hook(void* self, int32_t handle, uint32_t code, void* pdata, void* preply, uint32_t flags) {
    auto parcel = (PParcel*)pdata;
    detach(parcel, code);
    return transact_orig(self, handle, code, pdata, preply, flags);
}

static size_t read_companion(int fd) {
    off_t size = 0;
    if (!readFullFromFd(fd, &size, sizeof(size))) {
        LOGD("ERROR read_companion: truncated size header");
        return 0;
    }
    constexpr size_t MAX_CONFIG = 1024 * 1024;
    if (size <= 0 || static_cast<uint64_t>(size) > MAX_CONFIG) {
        LOGD("ERROR read_companion: invalid config size");
        return 0;
    }
    char* tmp = static_cast<char*>(malloc(static_cast<size_t>(size) + 1));
    if (!tmp) return 0;
    if (!readFullFromFd(fd, tmp, size)) {
        free(tmp);
        LOGD("ERROR read_companion: truncated payload");
        return 0;
    }
    if (!detach_codec::validate(tmp,static_cast<size_t>(size))) {
        LOGD("ERROR read_companion: invalid configuration encoding");
        free(tmp);
        return 0;
    }
    tmp[size] = 0;
    DETACH_TXT = tmp;
    DETACH_LEN = static_cast<size_t>(size);
    return DETACH_LEN;
}

static bool runPreSpecialize(const char* process, zygisk::Api* api) {
    if (strncmp(process, VENDING_PROC, STR_LEN(VENDING_PROC)) != 0) return false;

    int fd = api->connectCompanion();
    if (fd < 0) { LOGD("ERROR companion connection"); return false; }
    size_t detach_len = read_companion(fd);
    close(fd);
    if (detach_len == 0) return false;

    return true;
}

static bool runPostSpecialize(const char* process, zygisk::Api* api, JNIEnv* env) {
    int sdk = android_get_device_api_level();
    if (sdk <= 0) {
        LOGD("ERROR android_get_device_api_level: %d", sdk);
        return false;
    }
    HEADERS_LEN = getBinderHeadersLen(sdk);

    getApplicationEnabledSetting_code = getStaticIntFieldJni(env, STUB("android/content/pm/IPackageManager"),
                                                             TRSCTN("getApplicationEnabledSetting"));
    if (getApplicationEnabledSetting_code == 0) return false;

    ino_t inode;
    dev_t dev;
    if (!getMapping("libbinder.so", &inode, &dev)) {
        LOGD("ERROR: Could not get libbinder");
        return false;
    }

    api->pltHookRegister(dev, inode, "_ZN7android14IPCThreadState8transactEijRKNS_6ParcelEPS1_j",
                         (void**)&transact_hook, (void**)&transact_orig);
    if (!api->pltHookCommit()) {
        LOGD("ERROR: pltHookCommit");
        return false;
    }

    LOGD("HOOK_INSTALLED");
    return true;
}

class ZygiskDetach : public zygisk::ModuleBase {
   public:
    void onLoad(zygisk::Api* api, JNIEnv* env) override {
        this->api = api;
        this->env = env;
    }

    void preAppSpecialize(zygisk::AppSpecializeArgs* args) override {
        process = env->GetStringUTFChars(args->nice_name, nullptr);
        doRunPost = runPreSpecialize(process, api);

        if (!doRunPost) {
            cleanup(args);
        }
    }

    void postAppSpecialize(const zygisk::AppSpecializeArgs* args) override {
        if (!doRunPost) return;

        if (!runPostSpecialize(process, api, env)) {
            cleanup(args);
        }
    }

    void preServerSpecialize(zygisk::ServerSpecializeArgs* args) override {
        (void)args;
        api->setOption(zygisk::Option::DLCLOSE_MODULE_LIBRARY);
    }

    void cleanup(const zygisk::AppSpecializeArgs* args) {
        free(DETACH_TXT);
        DETACH_TXT = nullptr;
        DETACH_LEN = 0;
        env->ReleaseStringUTFChars(args->nice_name, process);
        api->setOption(zygisk::Option::DLCLOSE_MODULE_LIBRARY);
    }

   private:
    zygisk::Api* api;
    JNIEnv* env;

    bool doRunPost;
    const char* process;
};

static void companionHandler(int remote_fd) {
    companionSendFile("/data/adb/zygisk-detach/detach.bin", remote_fd);
}

REGISTER_ZYGISK_MODULE(ZygiskDetach)
REGISTER_ZYGISK_COMPANION(companionHandler)
