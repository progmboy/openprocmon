
#ifndef __THREAD_INCLUDE__H__
#define __THREAD_INCLUDE__H__

#include <stdint.h>
#include <windows.h>

class IRunObject
{
public:
    virtual void Run() = 0;
    virtual ~IRunObject(){}
    void* getParam()
    {
        return m_lpParam;
    }

    void setParam(void* lpParam)
    {
        m_lpParam = lpParam;
    }

	virtual bool IsStop()
	{
		return m_bIsStop;
	}
private:
    void* m_lpParam;

protected:
	bool m_bIsStop = false;
};


class CThread : public IRunObject
{
public:
    CThread();
    ~CThread();

    bool Start();
    void Stop();
    void TryJoin();
    void Join();
    void Kill();
	static DWORD WINAPI ThreadProc(void* lpParam);
    DWORD GetThreadID();
    bool IsAlive();
    bool IsTimeout();
	void SetTimeout(uint32_t timeout);

private:
    DWORD m_ThreadID = 0;
	HANDLE m_ThreadHandle = NULL;
    bool m_bIsStart = false;
    time_t m_startTime = 0;
    uint32_t m_TimeoutSeconds = 0;
};


#endif
