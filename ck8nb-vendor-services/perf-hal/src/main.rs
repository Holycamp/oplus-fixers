// IPerformance V4 stub HAL for Tecno CK8nB.
// The ROM's mediaserver (liboplusvideoboostservice) and system_server call this service. When it is
// missing, MediaPlayer.release()/video playback blocks in AServiceManager_getService and Settings'
// fingerprint enroll (guide video) hangs with an input ANR. Every method answers success with an
// empty/zero value; nothing is written to the kernel.
mod bindings;
mod logging;

use rsbinder::{hub, BinderResult, Interface, ProcessState};
use bindings::{BnPerformance, IPerformance};

const SERVICE_NAME: &str = "vendor.oplus.hardware.performance.IPerformance/default";

struct Perf;
impl Interface for Perf {}

impl IPerformance for Perf {
    fn addAcmDirName(&self, _dirname: &str, _flag: i64) -> BinderResult<i32> { Ok(0) }
    fn addAcmNomediaDirName(&self, _dirname: &str) -> BinderResult<i32> { Ok(0) }
    fn addAcmPkgName(&self, _pkgname: &str, _flag: i64) -> BinderResult<i32> { Ok(0) }
    fn addTaskTrackPid(&self, _group: i32, _pid: i32, _clear: bool) -> BinderResult<i32> { Ok(0) }
    fn clearTaskTrackGroup(&self, _group: i32) -> BinderResult<()> { Ok(()) }
    fn delAcmDirName(&self, _dirname: &str) -> BinderResult<i32> { Ok(0) }
    fn delAcmNomediaDirName(&self, _dirname: &str) -> BinderResult<i32> { Ok(0) }
    fn delAcmPkgName(&self, _pkgname: &str) -> BinderResult<i32> { Ok(0) }
    fn disableDamonReclaim(&self) -> BinderResult<i32> { Ok(0) }
    fn disableKmallocDebug(&self) -> BinderResult<i32> { Ok(0) }
    fn disableMultiThreadOptimize(&self) -> BinderResult<i32> { Ok(0) }
    fn disableProcessReclaim(&self) -> BinderResult<i32> { Ok(0) }
    fn disableTaskCpustats(&self) -> BinderResult<i32> { Ok(0) }
    fn disableTaskPlacementDecision(&self) -> BinderResult<i32> { Ok(0) }
    fn disableVmallocDebug(&self) -> BinderResult<i32> { Ok(0) }
    fn enableDamonReclaim(&self) -> BinderResult<i32> { Ok(0) }
    fn enableKmallocDebug(&self) -> BinderResult<i32> { Ok(0) }
    fn enableMultiThreadOptimize(&self) -> BinderResult<i32> { Ok(0) }
    fn enableProcessReclaim(&self) -> BinderResult<i32> { Ok(0) }
    fn enableTaskCpustats(&self) -> BinderResult<i32> { Ok(0) }
    fn enableTaskPlacementDecision(&self) -> BinderResult<i32> { Ok(0) }
    fn enableVmallocDebug(&self) -> BinderResult<i32> { Ok(0) }
    fn existMemMonitor(&self) -> BinderResult<i32> { Ok(0) }
    fn getAcmDirFlag(&self, _dirname: &str) -> BinderResult<i64> { Ok(0) }
    fn getAcmOpstat(&self) -> BinderResult<i32> { Ok(0) }
    fn getAcmPkgFlag(&self, _pkgname: &str) -> BinderResult<i64> { Ok(0) }
    fn getDdrResidency(&self) -> BinderResult<String> { Ok(String::new()) }
    fn getDevinfoUfsInfo(&self) -> BinderResult<String> { Ok(String::new()) }
    fn getDevinfoDDRInfo(&self) -> BinderResult<String> { Ok(String::new()) }
    fn getDevinfoUfsVersionInfo(&self) -> BinderResult<String> { Ok(String::new()) }
    fn getExt4FragScore(&self, _devpath: &str) -> BinderResult<String> { Ok(String::new()) }
    fn getExt4FreefragInfo(&self, _devpath: &str) -> BinderResult<String> { Ok(String::new()) }
    fn getF2fsMovedBlks(&self) -> BinderResult<String> { Ok(String::new()) }
    fn getHIAllocWait(&self) -> BinderResult<bindings::vendor::oplus::hardware::performance::ProcReqHal::ProcReqHal> { Ok(bindings::vendor::oplus::hardware::performance::ProcReqHal::ProcReqHal::default()) }
    fn getHICpuInfo(&self) -> BinderResult<String> { Ok(String::new()) }
    fn getHICpuLoading(&self) -> BinderResult<String> { Ok(String::new()) }
    fn getHIDState(&self) -> BinderResult<bindings::vendor::oplus::hardware::performance::ProcReqHal::ProcReqHal> { Ok(bindings::vendor::oplus::hardware::performance::ProcReqHal::ProcReqHal::default()) }
    fn getHIEmcdrvIowait(&self) -> BinderResult<bindings::vendor::oplus::hardware::performance::ProcReqHal::ProcReqHal> { Ok(bindings::vendor::oplus::hardware::performance::ProcReqHal::ProcReqHal::default()) }
    fn getHIFsyncWait(&self) -> BinderResult<bindings::vendor::oplus::hardware::performance::ProcReqHal::ProcReqHal> { Ok(bindings::vendor::oplus::hardware::performance::ProcReqHal::ProcReqHal::default()) }
    fn getHIIonWait(&self) -> BinderResult<bindings::vendor::oplus::hardware::performance::ProcReqHal::ProcReqHal> { Ok(bindings::vendor::oplus::hardware::performance::ProcReqHal::ProcReqHal::default()) }
    fn getHIIowait(&self) -> BinderResult<bindings::vendor::oplus::hardware::performance::ProcReqHal::ProcReqHal> { Ok(bindings::vendor::oplus::hardware::performance::ProcReqHal::ProcReqHal::default()) }
    fn getHIIowaitHung(&self) -> BinderResult<bindings::vendor::oplus::hardware::performance::ProcReqHal::ProcReqHal> { Ok(bindings::vendor::oplus::hardware::performance::ProcReqHal::ProcReqHal::default()) }
    fn getHIKswapdLoading(&self) -> BinderResult<bindings::vendor::oplus::hardware::performance::ProcReqHal::ProcReqHal> { Ok(bindings::vendor::oplus::hardware::performance::ProcReqHal::ProcReqHal::default()) }
    fn getHISchedLatency(&self) -> BinderResult<bindings::vendor::oplus::hardware::performance::ProcReqHal::ProcReqHal> { Ok(bindings::vendor::oplus::hardware::performance::ProcReqHal::ProcReqHal::default()) }
    fn getHIScmCall(&self) -> BinderResult<bindings::vendor::oplus::hardware::performance::ProcReqHal::ProcReqHal> { Ok(bindings::vendor::oplus::hardware::performance::ProcReqHal::ProcReqHal::default()) }
    fn getHIUfsFeature(&self) -> BinderResult<bindings::vendor::oplus::hardware::performance::ProcReqHal::ProcReqHal> { Ok(bindings::vendor::oplus::hardware::performance::ProcReqHal::ProcReqHal::default()) }
    fn getKernelVersion(&self) -> BinderResult<i32> { Ok(0) }
    fn getKmallocDebug(&self) -> BinderResult<String> { Ok(String::new()) }
    fn getKmallocOrigin(&self) -> BinderResult<String> { Ok(String::new()) }
    fn getKmallocUsed(&self) -> BinderResult<String> { Ok(String::new()) }
    fn getMemMonitor(&self) -> BinderResult<String> { Ok(String::new()) }
    fn getOswapVersion(&self) -> BinderResult<i32> { Ok(0) }
    fn getUfsSignalRecordUpload(&self) -> BinderResult<String> { Ok(String::new()) }
    fn getUfsplusHpbStatus(&self) -> BinderResult<i32> { Ok(0) }
    fn getUfsplusTwStatus(&self) -> BinderResult<i32> { Ok(0) }
    fn getVmallocDebug(&self) -> BinderResult<String> { Ok(String::new()) }
    fn getVmallocHashCal(&self) -> BinderResult<String> { Ok(String::new()) }
    fn getVmallocUsed(&self) -> BinderResult<String> { Ok(String::new()) }
    fn getallocwait(&self) -> BinderResult<String> { Ok(String::new()) }
    fn getdstate(&self) -> BinderResult<String> { Ok(String::new()) }
    fn getfsyncwait(&self) -> BinderResult<String> { Ok(String::new()) }
    fn getionwait(&self) -> BinderResult<String> { Ok(String::new()) }
    fn getiowait(&self) -> BinderResult<String> { Ok(String::new()) }
    fn getschedlatency(&self) -> BinderResult<String> { Ok(String::new()) }
    fn hybridswap_memcg_para_read(&self, _action: i32, _cgroup: &str) -> BinderResult<String> { Ok(String::new()) }
    fn hybridswap_memcg_para_write(&self, _action: i32, _cgroup: &str, _str: &str) -> BinderResult<i32> { Ok(0) }
    fn hybridswap_zram_para_read(&self, _action: i32) -> BinderResult<String> { Ok(String::new()) }
    fn hybridswap_zram_para_write(&self, _action: i32, _str: &str) -> BinderResult<i32> { Ok(0) }
    fn isJankTaskTrackEnable(&self) -> BinderResult<bool> { Ok(false) }
    fn perProcessMemReadahead(&self, _uid: i32, _pid: i32, _type: i32) -> BinderResult<i32> { Ok(0) }
    fn perProcessMemReclaim(&self, _uid: i32, _pid: i32, _type: i32) -> BinderResult<i32> { Ok(0) }
    fn readCallStack(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readClmEnable(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readClmHighLoadAll(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readClmHighLoadGrp(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readClmLowLoadGrp(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readCpuTaskstats(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readDBacktrace(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readDConvert(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readFgFreqsThreshold(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readIOBacktrace(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readIomonitorInfo(&self, _procname: &str) -> BinderResult<String> { Ok(String::new()) }
    fn readJankCpuIndicator(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readJankCpuInfo(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readJankCpuInfoSig(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readJankCpuLoad(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readJankCpuLoad32(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readJankCpuLoad32Scale(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readJankTaskTrack(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readJankTaskTrackByPid(&self, _pid: i32) -> BinderResult<String> { Ok(String::new()) }
    fn readJankVersion(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readKmallocDebugCreate(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readLimitTable(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readMemleakDetectThread(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readMemoryByPids(&self, _pids: &[i32], _flags: i32) -> BinderResult<bindings::vendor::oplus::hardware::performance::ProcMemStatRet::ProcMemStatRet> { Ok(bindings::vendor::oplus::hardware::performance::ProcMemStatRet::ProcMemStatRet::default()) }
    fn readMemoryByUids(&self, _uids: &[i32], _flags: i32) -> BinderResult<bindings::vendor::oplus::hardware::performance::ProcMemStatRet::ProcMemStatRet> { Ok(bindings::vendor::oplus::hardware::performance::ProcMemStatRet::ProcMemStatRet::default()) }
    fn readNandswapProc(&self, _inProc: &str) -> BinderResult<String> { Ok(String::new()) }
    fn readNormalizeRealTime(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readNormalizeRunningTime(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readOplusReserve3(&self, _offset: i32, _len: i32) -> BinderResult<String> { Ok(String::new()) }
    fn readOsvelteVersion(&self) -> BinderResult<bindings::vendor::oplus::hardware::performance::OsvelteVersionRet::OsvelteVersionRet> { Ok(bindings::vendor::oplus::hardware::performance::OsvelteVersionRet::OsvelteVersionRet::default()) }
    fn readPidsSet(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readRealTime(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readRunningTime(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readSchedInfoThreshold(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readSgeFreqInfo(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readSgeInfo(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readStorageFeature(&self, _name: &str, _addr: &str, _isMulti: &str) -> BinderResult<String> { Ok(String::new()) }
    fn readTargetProcess(&self, _buffer: &str) -> BinderResult<String> { Ok(String::new()) }
    fn readTaskCpustatsEnable(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readTaskSchedInfo(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readTidsSet(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readUxTaskTrack(&self, _uPid: i32, _rPid: i32) -> BinderResult<String> { Ok(String::new()) }
    fn readVaFeature(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readVersion(&self) -> BinderResult<String> { Ok(String::new()) }
    fn removeTaskTrackPid(&self, _group: i32, _pid: i32) -> BinderResult<()> { Ok(()) }
    fn searchAcmNomediaDirName(&self, _dirname: &str) -> BinderResult<i32> { Ok(0) }
    fn setAcmOpstat(&self, _flag: i32) -> BinderResult<i32> { Ok(0) }
    fn setDamonReclaimColdTime(&self, _cold_time: i32) -> BinderResult<i32> { Ok(0) }
    fn setDamonReclaimMonitoring(&self, _sample: i32, _aggr: i32) -> BinderResult<i32> { Ok(0) }
    fn setDamonReclaimQuota(&self, _quota_ms: i32, _quota_sz: i32, _reset_interval: i32) -> BinderResult<i32> { Ok(0) }
    fn setDamonReclaimWmarks(&self, _metric: i32, _high: i32, _mid: i32, _low: i32) -> BinderResult<i32> { Ok(0) }
    fn setFgUids(&self, _fg_uid: &str) -> BinderResult<()> { Ok(()) }
    fn setFrameRate(&self, _frame_rate: &str) -> BinderResult<()> { Ok(()) }
    fn setImFlag(&self, _pid: &str, _im_flag: &str) -> BinderResult<()> { Ok(()) }
    fn setProcessReclaim(&self, _info: &str) -> BinderResult<i32> { Ok(0) }
    fn setSchedAssistImptTask(&self, _impt_info: &str) -> BinderResult<()> { Ok(()) }
    fn setSchedAssistScene(&self, _scene_id: &str) -> BinderResult<()> { Ok(()) }
    fn setSlideboost(&self, _boost: &str) -> BinderResult<()> { Ok(()) }
    fn setTpdID(&self, _param: &str) -> BinderResult<i32> { Ok(0) }
    fn setTpdSerialParams(&self, _params: &str) -> BinderResult<i32> { Ok(0) }
    fn writeClmEnable(&self, _buffer: &str) -> BinderResult<()> { Ok(()) }
    fn writeClmHighLoadAll(&self, _buffer: &str) -> BinderResult<()> { Ok(()) }
    fn writeClmHighLoadGrp(&self, _buffer: &str) -> BinderResult<()> { Ok(()) }
    fn writeClmLowLoadGrp(&self, _buffer: &str) -> BinderResult<()> { Ok(()) }
    fn writeDBacktrace(&self, _buffer: &str) -> BinderResult<()> { Ok(()) }
    fn writeFgFreqsThreshold(&self, _buffer: &str) -> BinderResult<()> { Ok(()) }
    fn writeIOBacktrace(&self, _buffer: &str) -> BinderResult<()> { Ok(()) }
    fn writeJankTaskTrackEnable(&self, _enable: bool) -> BinderResult<()> { Ok(()) }
    fn writeKmallocDebugCreate(&self, _kcreate: i32) -> BinderResult<i32> { Ok(0) }
    fn writeKmallocDebugCreateWithType(&self, _type: &str) -> BinderResult<i32> { Ok(0) }
    fn writeMemMonitor(&self, _buffer: &str) -> BinderResult<i32> { Ok(0) }
    fn writeMemleakDetectThread(&self, _memdect: i32) -> BinderResult<i32> { Ok(0) }
    fn writeMonitorStatus(&self, _buffer: &str) -> BinderResult<i32> { Ok(0) }
    fn writeNandswapProc(&self, _inProc: &str, _cmd: &str) -> BinderResult<i32> { Ok(0) }
    fn writeOplusReserve3(&self, _offset: i32, _len: i32, _info: &str) -> BinderResult<i32> { Ok(0) }
    fn writePidsSet(&self, _buffer: &str) -> BinderResult<()> { Ok(()) }
    fn writeSchedInfoThreshold(&self, _buffer: &str) -> BinderResult<()> { Ok(()) }
    fn writeStorageFeature(&self, _name: &str, _addr: &str, _isMulti: &str, _cmd: &str) -> BinderResult<i32> { Ok(0) }
    fn writeTaskSchedInfo(&self, _buffer: &str) -> BinderResult<()> { Ok(()) }
    fn writeTidsSet(&self, _buffer: &str) -> BinderResult<()> { Ok(()) }
    fn writeUxState(&self, _ux_state: &str, _pid: &str, _tid: &str) -> BinderResult<i32> { Ok(0) }
    fn writeVaFeature(&self, _vafeature: i32) -> BinderResult<i32> { Ok(0) }
    fn setFreqGoverner(&self, _gov_name: &str, _clusters: &[i32]) -> BinderResult<i32> { Ok(0) }
    fn enableAudioPerf(&self, _value: &str) -> BinderResult<i32> { Ok(0) }
    fn writeTmemoryMemory(&self, _str: &str) -> BinderResult<i32> { Ok(0) }
    fn writeTmemorySwitch(&self, _param: i32) -> BinderResult<i32> { Ok(0) }
    fn writeTmemoryFlushBusy(&self, _param: i32) -> BinderResult<i32> { Ok(0) }
    fn writeTmemoryFlushIdle(&self, _param: i32) -> BinderResult<i32> { Ok(0) }
    fn writeTmemoryCapacity(&self, _param: i32) -> BinderResult<i32> { Ok(0) }
    fn writeTmemoryHighWaterRatio(&self, _param: i32) -> BinderResult<i32> { Ok(0) }
    fn readTmemorySysdirtypages(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readTmemoryDirtypages(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readTmemoryErrorStat(&self) -> BinderResult<String> { Ok(String::new()) }
    fn readTmemoryIoLatency(&self) -> BinderResult<String> { Ok(String::new()) }
    fn getClmThreshold(&self, _threshold_id: i32) -> BinderResult<String> { Ok(String::new()) }
    fn getClmMuxSwitch(&self) -> BinderResult<String> { Ok(String::new()) }
    fn setClmThreshold(&self, _buffer: &str, _threshold_id: i32) -> BinderResult<()> { Ok(()) }
    fn setClmMuxSwitch(&self, _buffer: &str) -> BinderResult<()> { Ok(()) }
    fn setWakeSeedThread(&self, _tid: &str, _identify_type: bool, _inUid: bool) -> BinderResult<i32> { Ok(0) }
    fn getWakeThreadsAffinityOrdered(&self, _handle: &str, _size: i32) -> BinderResult<bindings::vendor::oplus::hardware::performance::TaskWakeInfo::TaskWakeInfo> { Ok(bindings::vendor::oplus::hardware::performance::TaskWakeInfo::TaskWakeInfo::default()) }
    fn setExtSchedProp(&self, _pid: &str, _prop: &str) -> BinderResult<()> { Ok(()) }
    fn readMMParameters(&self, _cmd: bindings::vendor::oplus::hardware::performance::MmWatermarkScale::MmWatermarkScale) -> BinderResult<String> { Ok(String::new()) }
    fn writeMMParameters(&self, _cmd: bindings::vendor::oplus::hardware::performance::MmWatermarkScale::MmWatermarkScale, _val: &str) -> BinderResult<i32> { Ok(0) }
}

fn main() {
    logging::init();
    logging::init_rsbinder_logger();
    if let Err(e) = ProcessState::init_default() {
        logging::error(&format!("binder init failed: {e:?}"));
        std::process::exit(1);
    }
    ProcessState::start_thread_pool();
    let binder = BnPerformance::new_binder(Perf);
    match hub::add_service(SERVICE_NAME, binder.as_binder()) {
        Ok(()) => logging::info(&format!("registered {SERVICE_NAME}")),
        Err(e) => {
            logging::error(&format!("register failed: {e:?}"));
            std::process::exit(1);
        }
    }
    let _ = ProcessState::join_thread_pool();
}
