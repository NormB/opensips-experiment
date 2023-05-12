// These were copy/pasted from an arbitrary build of the `options`
// module.
//
// TODO: These need to be pulled from the version of OpenSIPS we are
// building against.
#define PIC
#define MOD_NAME "options"
#define PKG_MALLOC
#define SHM_MMAP
#define USE_MCAST
#define DISABLE_NAGLE
#define STATISTICS
#define HAVE_RESOLV_RES
#define F_MALLOC
#define Q_MALLOC
#define HP_MALLOC
#define DBG_MALLOC
#define CC_O0
#define HAVE_STDATOMIC
#define HAVE_GENERICS
#define NAME "opensips"
#define VERSION "3.4.0-dev"
#define ARCH "aarch64"
#define OS "linux"
#define COMPILER "gcc 11"
#define __CPU_aarch64
#define __OS_linux
#define __SMP_yes
#define CFG_DIR "/usr/local/etc/opensips/"
#define VERSIONTYPE "git"
#define THISREVISION "5fc57e944"
#define HAVE_GETHOSTBYNAME2
#define HAVE_UNION_SEMUN
#define HAVE_SCHED_YIELD
#define HAVE_MSG_NOSIGNAL
#define HAVE_MSGHDR_MSG_CONTROL
#define HAVE_ALLOCA_H
#define HAVE_TIMEGM
#define USE_POSIX_SEM
#define HAVE_EPOLL
#define HAVE_SIGIO_RT
#define HAVE_SELECT

#include "sr_module.h"
#include "modules/signaling/signaling.h"
#include "data_lump_rpl.h"
