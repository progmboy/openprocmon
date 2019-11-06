#pragma once

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
#include "strmaps.hpp"

#include "monctl.hpp"
#include "eventmgr.hpp"
#include "eventview.hpp"