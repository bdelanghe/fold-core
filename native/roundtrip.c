// The native library from C, as a host links it (checks.native): each stdin line is `<call>\t<input JSON>`, each stdout line
// the call's output. The host's half of the ABI too: fold_core_tz_offset from the system's zone rules (TZDIR), INT32_MIN for a
// zone it does not have. native/roundtrip.ts feeds it every golden and compares.
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

#include "fold_core.h"

int32_t fold_core_tz_offset(const uint8_t *zone, uintptr_t len, double epoch_ms) {
  char name[256], path[1024];
  if (len >= sizeof name) return INT32_MIN;
  memcpy(name, zone, len);
  name[len] = 0;
  const char *dir = getenv("TZDIR");
  snprintf(path, sizeof path, "%s/%s", dir ? dir : "/usr/share/zoneinfo", name);
  if (strstr(name, "..") || access(path, R_OK) != 0) return INT32_MIN;
  setenv("TZ", name, 1);
  tzset();
  time_t t = (time_t)(epoch_ms / 1000);
  struct tm tm;
  localtime_r(&t, &tm);
  return (int32_t)tm.tm_gmtoff;
}

int main(void) {
  char *line = NULL;
  size_t cap = 0;
  ssize_t n;
  while ((n = getline(&line, &cap, stdin)) > 0) {
    if (line[n - 1] == '\n') line[--n] = 0;
    char *tab = strchr(line, '\t');
    if (!tab) return 2;
    *tab = 0;
    size_t len = strlen(tab + 1);
    uint8_t *in = fold_core_alloc(len);
    memcpy(in, tab + 1, len);
    FoldCoreBuf out = !strcmp(line, "fold")    ? fold_core_fold_json(in, len)
                      : !strcmp(line, "walk")  ? fold_core_walk_json(in, len)
                      : !strcmp(line, "ax")    ? fold_core_ax_json(in, len)
                      : !strcmp(line, "rotor") ? fold_core_rotor_json(in, len)
                                               : (FoldCoreBuf){0};
    if (!out.ptr) return 2;
    fwrite(out.ptr, 1, out.len, stdout);
    fputc('\n', stdout);
    fold_core_free(out.ptr, out.len);
  }
  free(line);
  return 0;
}
