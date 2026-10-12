#ifndef MOTE_IOS_H
#define MOTE_IOS_H
#include <TargetConditionals.h>
#include <stddef.h>
#include <stdint.h>

#if TARGET_OS_SIMULATOR
const char *mote_ios_probe_documents(void);
#endif

__attribute__((noreturn)) void mote_ios_main(void);
void mote_ios_attach_view(void *view);
void mote_ios_accessibility_publish(const uint8_t *bytes, size_t length);
void mote_ios_accessibility_action(uint64_t id, uint8_t kind);
typedef struct {
    double top;
    double left;
    double bottom;
    double right;
} MoteInsets;
MoteInsets mote_ios_safe_area(void);

#endif
