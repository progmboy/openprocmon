
#include "pch.hpp"

#include "logger.hpp"
#include <mutex>

std::mutex gLogLock;

BOOL LogMessage(LEVEL Level, LPCTSTR Format, ...)
{
	TCHAR Buffer[MAX_LOG_MESSAGE] = { 0 };
	va_list Args;

	va_start(Args, Format);
	StringCchVPrintf(Buffer, MAX_LOG_MESSAGE, Format, Args);
	va_end(Args);

	gLogLock.lock();

	switch (Level) {
	case L_DEBUG: _ftprintf(stdout, TEXT("[?] %s\n"), Buffer); break;
	case L_INFO:  _ftprintf(stdout, TEXT("[+] %s\n"), Buffer); break;
	case L_WARN:  _ftprintf(stderr, TEXT("[*] %s\n"), Buffer); break;
	case L_ERROR: _ftprintf(stderr, TEXT("[!] %s\n"), Buffer); break;
	}

	fflush(stdout);
	fflush(stderr);

	gLogLock.unlock();

	return TRUE;
}