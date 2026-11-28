// face-probe: loads the stock Transsion legacy face HAL (/vendor/lib64/hw/face.default.so),
// drives it directly and prints every callback message as raw hex. It does NOT touch
// system_server or the AIDL stub. Purpose: confirm the engine runs on this ROM and record
// the exact face_msg_t layout so a real AIDL face HAL can be written against facts.
//
// Layout of the face device (decoded from face.default.so, aarch64):
//   0x00 hw_device_t (0x78 bytes, close() at 0x70)
//   0x80 notify callback storage      0x88 set_notify(dev, cb)
//   0x90 pre_enroll   0x98 enroll   0xa0 ?   0xa8 set_feature  0xb0 get_feature
//   0xb8 get_authenticator_id  0xc0 cancel  0xc8 enumerate  0xd0 remove
//   0xd8 set_active_group  0xe0 authenticate  0xe8 (null)  0xf0 reset_lockout
// Callback: void cb(const void *msg) ; msg is 0x58 bytes, int32 type at +0 (-1 = error),
// payload from +8 (cancel sends type -1, code 5).
#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

extern int __android_log_print(int prio, const char *tag, const char *fmt, ...);
#define LOG(...) do { printf(__VA_ARGS__); printf("\n"); fflush(stdout); \
                      __android_log_print(4, "FaceProbe", __VA_ARGS__); } while (0)

typedef struct { uint32_t tag, version; void *module; uint64_t reserved[12];
                 int (*close)(void *); } hw_device_t;
typedef struct { uint32_t tag; uint16_t module_api_version, hal_api_version;
                 const char *id, *name, *author;
                 struct { int (*open)(const void *, const char *, void **); } *methods;
                 void *dso; uint64_t reserved[25]; } hw_module_t;

#define SLOT(dev, off) (*(void **)((char *)(dev) + (off)))

static void on_msg(const void *m) {
    const uint8_t *b = m;
    char hex[0x58 * 3 + 1];
    for (int i = 0; i < 0x58; i++) sprintf(hex + i * 3, "%02x ", b[i]);
    LOG("MSG type=%d a=%d b=%d c=%d | %s", ((const int32_t *)m)[0], ((const int32_t *)m)[2],
        ((const int32_t *)m)[3], ((const int32_t *)m)[4], hex);
}

int main(int argc, char **argv) {
    const char *cmd = argc > 1 ? argv[1] : "info";
    int secs = argc > 2 ? atoi(argv[2]) : 30;
    const char *path = "/vendor/lib64/hw/face.default.so";
    void *h = dlopen(path, RTLD_NOW);
    if (!h) { LOG("dlopen failed: %s", dlerror()); return 1; }
    hw_module_t *mod = dlsym(h, "HMI");
    if (!mod) { LOG("no HMI symbol"); return 1; }
    LOG("module id=%s name=%s author=%s api=%x", mod->id, mod->name, mod->author, mod->module_api_version);
    void *dev = NULL;
    int r = mod->methods->open(mod, "face", &dev);
    LOG("open -> %d dev=%p", r, dev);
    if (r || !dev) return 1;

    int (*set_notify)(void *, void (*)(const void *)) = SLOT(dev, 0x88);
    int (*pre_enroll)(void *, uint64_t *) = SLOT(dev, 0x90);
    int (*enroll)(void *, const uint8_t *, int, const uint32_t *, size_t) = SLOT(dev, 0x98);
    int (*get_auth_id)(void *, uint64_t *) = SLOT(dev, 0xb8);
    int (*cancel)(void *) = SLOT(dev, 0xc0);
    int (*enumerate)(void *) = SLOT(dev, 0xc8);
    int (*set_active_group)(void *, int, const char *) = SLOT(dev, 0xd8);
    int (*authenticate)(void *, uint64_t, int) = SLOT(dev, 0xe0);

    LOG("set_notify -> %d", set_notify(dev, on_msg));
    LOG("set_active_group -> %d", set_active_group(dev, 0, "/data/vendor_de/0/facedata"));
    uint64_t aid = 0;
    LOG("get_authenticator_id -> %d id=%llu", get_auth_id(dev, &aid), (unsigned long long)aid);

    if (!strcmp(cmd, "enroll")) {
        uint64_t ch = 0;
        int pr = pre_enroll(dev, &ch);
        LOG("pre_enroll -> %d challenge=%llu", pr, (unsigned long long)ch);
        uint8_t hat[69] = {0};
        memcpy(hat + 1, &ch, 8);               // hw_auth_token.challenge lives at offset 1
        LOG("enroll -> %d", enroll(dev, hat, secs, NULL, 0));
        LOG("look at the front camera now; waiting %d s for messages", secs);
        sleep(secs + 3);
        cancel(dev);
    } else if (!strcmp(cmd, "auth")) {
        LOG("authenticate -> %d", authenticate(dev, 0, 0));
        sleep(secs);
        cancel(dev);
    } else if (!strcmp(cmd, "enum")) {
        LOG("enumerate -> %d", enumerate(dev));
        sleep(2);
    }
    LOG("done");
    return 0;
}
