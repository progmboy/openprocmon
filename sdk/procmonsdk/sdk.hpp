#pragma once

#include <stdio.h>
#include <tchar.h>
#include <strsafe.h>
#include <windows.h>
#include <winternl.h>
#include <fltUser.h>
#include <atlstr.h>
#include <atlpath.h>

typedef enum _KEY_INFORMATION_CLASS {
	KeyBasicInformation,
	KeyNodeInformation,
	KeyFullInformation,
	KeyNameInformation,
	KeyCachedInformation,
	KeyFlagsInformation,
	KeyVirtualizationInformation,
	KeyHandleTagsInformation,
	KeyTrustInformation,
	KeyLayerInformation,
	MaxKeyInfoClass  // MaxKeyInfoClass should always be the last enum
} KEY_INFORMATION_CLASS;

typedef enum _KEY_VALUE_INFORMATION_CLASS {
	KeyValueBasicInformation,
	KeyValueFullInformation,
	KeyValuePartialInformation,
	KeyValueFullInformationAlign64,
	KeyValuePartialInformationAlign64,
	KeyValueLayerInformation,
	MaxKeyValueInfoClass  // MaxKeyValueInfoClass should always be the last enum
} KEY_VALUE_INFORMATION_CLASS;

#include "../../kernel/logsdk.h"

#include "singleton.hpp"
#include "refobject.hpp"
#include "drvload.hpp"
#include "buffer.hpp"
#include "utils.hpp"
#include "thread.hpp"
#include "logger.hpp"
#include "utils.hpp"

#include "monctl.hpp"
#include "eventmgr.hpp"
#include "eventview.hpp"


