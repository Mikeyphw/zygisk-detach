LOCAL_PATH := $(call my-dir)
include $(CLEAR_VARS)
LOCAL_MODULE := zygisk_detach
LOCAL_SRC_FILES := module.cpp binder.cpp
LOCAL_CPPFLAGS := -std=c++17 -Wall -Wextra -fvisibility=hidden
LOCAL_LDLIBS := -llog
include $(BUILD_SHARED_LIBRARY)
