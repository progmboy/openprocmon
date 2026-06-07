#pragma once

#include <windows.h>

typedef enum { L_DEBUG, L_INFO, L_WARN, L_ERROR } LEVEL, *PLEVEL;


//
// A quick logging routine for debug messages.
//

#define MAX_LOG_MESSAGE 1024
BOOL LogMessage(LEVEL Level, LPCTSTR Format, ...);