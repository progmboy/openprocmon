#pragma once

#include "pch.hpp"
#include "procmgr.hpp"
#include "regopt.hpp"
#include "eventview.hpp"
#include "strmaps.hpp"


template <class T>
CString GetRegKeyPath(PLOG_ENTRY pEntry)
{
    CString strRegPath;
    T pInfo = TO_EVENT_DATA(T, pEntry);
    if (pInfo->KeyNameLength) {
        CString strRegPathInternal;
        strRegPathInternal.Append((LPCWSTR)(pInfo + 1), pInfo->KeyNameLength);
        UtilConvertRegInternalToNormal(strRegPathInternal, strRegPath);
    }
    
    return strRegPath;
}

CString CRegEvent::GetPath()
{
    PLOG_ENTRY pEntry = reinterpret_cast<PLOG_ENTRY>(getPreLog().GetBuffer());

    switch (pEntry->NotifyType)
    {
    case NOTIFY_REG_CREATEKEYEX:
    case NOTIFY_REG_OPENKEYEX:
    {
        return GetRegKeyPath<PLOG_REG_CREATEOPENKEY>(pEntry);
    }

    case NOTIFY_REG_QUERYVALUEKEY:
    {
        return GetRegKeyPath<PLOG_REG_QUERYVALUEKEY>(pEntry);
    }
    case NOTIFY_REG_ENUMERATEVALUEKEY:
    {
        return GetRegKeyPath<PLOG_REG_ENUMERATEVALUEKEY>(pEntry);
    }
    case NOTIFY_REG_ENUMERATEKEY:
    {
        return GetRegKeyPath<PLOG_REG_ENUMERATEKEY>(pEntry);
    }
    case NOTIFY_REG_SETINFORMATIONKEY:
    {
        return GetRegKeyPath<PLOG_REG_SETINFORMATIONKEY>(pEntry);
    }
    case NOTIFY_REG_DELETEVALUEKEY:
    {
        return GetRegKeyPath<PLOG_REG_DELETEVALUEKEY>(pEntry);
    }
    case NOTIFY_REG_QUERYKEY:
    {
        return GetRegKeyPath<PLOG_REG_QUERYKEY>(pEntry);
    }

    case NOTIFY_REG_LOADKEY:
    {
        return GetRegKeyPath<PLOG_REG_LOADKEY>(pEntry);
    }

    case NOTIFY_REG_UNLOADKEY:
    {
        return GetRegKeyPath<PLOG_REG_UNLOADKEY>(pEntry);
    }

    case NOTIFY_REG_RENAMEKEY:
    {
        return GetRegKeyPath<PLOG_REG_RENAMEKEY>(pEntry);
    }

    case NOTIFY_REG_SETVALUEKEY:
    {
        return GetRegKeyPath<PLOG_REG_SETVALUEKEY>(pEntry);
    }
    case NOTIFY_REG_SETKEYSECURITY:
    case NOTIFY_REG_QUERYMULTIPLEVALUEKEY:
    case NOTIFY_REG_FLUSHKEY:
    case NOTIFY_REG_DELETEKEY:
    case NOTIFY_REG_KEYHANDLECLOSE:
    case NOTIFY_REG_QUERYKEYSECURITY:
    {
        return GetRegKeyPath<PLOG_REG_CONNMON>(pEntry);
    }
    default:
        break;
    }

    return TEXT("");
}