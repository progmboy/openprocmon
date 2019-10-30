// pch.h: This is a precompiled header file.
// Files listed below are compiled only once, improving build performance for future builds.
// This also affects IntelliSense performance, including code completion and many code browsing features.
// However, files listed here are ALL re-compiled if any one of them is updated between builds.
// Do not add files here that you will be updating frequently as this negates the performance advantage.

#ifndef PCH_H
#define PCH_H

#include <stdio.h>
#include <tchar.h>
#include <strsafe.h>
#include <atlstr.h>
#include <atlpath.h>

#include <map>
#include <vector>
#include <mutex>
#include <condition_variable>
#include <queue>

#include "kernelsdk.hpp"
#include "singleton.hpp"
#include "refobject.hpp"
#include "drvload.hpp"
#include "buffer.hpp"
#include "utils.hpp"
#include "thread.hpp"
#include "logger.hpp"
#include "utils.hpp"

#endif //PCH_H
