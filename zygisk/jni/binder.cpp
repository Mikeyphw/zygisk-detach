#include "binder.hpp"

#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/sendfile.h>
#include <sys/stat.h>
#include <sys/sysmacros.h>
#include <unistd.h>

#include "module.hpp"
#include "stream.hpp"

bool getMapping(const char* lib_name, ino_t* inode, dev_t* dev) {
    FILE* fp = fopen("/proc/self/maps", "r");
    if (!fp) return false;
    char mapbuf[256], flags[8];
    int lib_name_len = strlen(lib_name);
    while (fgets(mapbuf, sizeof(mapbuf), fp)) {
        unsigned int dev_major, dev_minor;
        int cur = 0;
        unsigned long long inode_value = 0;
        if (sscanf(mapbuf, "%*s %7s %*x %x:%x %llu %*s%n", flags,
                   &dev_major, &dev_minor, &inode_value, &cur) != 4) continue;
        *inode = static_cast<ino_t>(inode_value);
        if (cur < lib_name_len) continue;
        if (memcmp(&mapbuf[cur - lib_name_len], lib_name, lib_name_len) == 0 && flags[2] == 'x') {
            *dev = makedev(dev_major, dev_minor);
            fclose(fp);
            return true;
        }
    }
    fclose(fp);
    return false;
}

uint32_t getStaticIntFieldJni(JNIEnv* env, const char* cls_name, const char* field_name) {
    jclass cls = env->FindClass(cls_name);
    if (cls == nullptr) {
        env->ExceptionClear();
        LOGD("ERROR getStaticIntFieldJni: Could not get class '%s'", cls_name);
        return 0;
    }
    jfieldID field = env->GetStaticFieldID(cls, field_name, "I");
    if (field == nullptr) {
        env->ExceptionClear();
        LOGD("ERROR getStaticIntFieldJni: Could not get field %s.%s", cls_name, field_name);
        return 0;
    }
    jint val = env->GetStaticIntField(cls, field);
    return val;
}

void companionSendFile(const char* path, int remote_fd) {
    off_t size = 0;
    int fd = open(path, O_RDONLY);
    if (fd == -1) {
        LOGD("ERROR open: %s", strerror(errno));
        goto defer;
    }

    struct stat st;
    if (fstat(fd, &st) == -1) {
        LOGD("ERROR fstat: %s", strerror(errno));
        goto defer;
    }
    size = st.st_size;

defer:
    if (!detach_io::write_full(remote_fd, &size, sizeof(size))) {
        LOGD("ERROR write: %s", strerror(errno));
        size = 0;
    }
    if (fd >= 0) {
        off_t offset = 0;
        while (size > 0 && offset < size) {
            ssize_t sent = sendfile(remote_fd, fd, &offset, static_cast<size_t>(size - offset));
            if (sent == 0) { LOGD("ERROR sendfile: unexpected EOF"); break; }
            if (sent < 0) {
                if (errno == EINTR) continue;
                LOGD("ERROR sendfile: %s", strerror(errno));
                break;
            }
        }
        close(fd);
    }
}

bool readFullFromFd(int fd, void* buf, off_t size) {
    return size >= 0 && detach_io::read_full(fd, buf, static_cast<size_t>(size));
}
