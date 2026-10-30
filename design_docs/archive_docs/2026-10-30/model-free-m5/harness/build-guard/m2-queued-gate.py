import datetime,hashlib,json,os,pathlib,subprocess,sys,time
base=pathlib.Path(__file__).resolve().parent
name,cwd,*command=sys.argv[1:]
assert not (base/(name+'.json')).exists()
wrapped=['flock','-w','600','/home/markik/Code/testing/machine-heavy.lock','systemd-run','--user','--scope','--unit=custody-contract-'+name,'--property=MemoryMax=3G','--property=MemorySwapMax=0','--property=CPUQuota=150%','env','CARGO_TARGET_DIR=/home/markik/Code/target','CARGO_BUILD_BUILD_DIR=/home/markik/Code/target','CARGO_BUILD_JOBS=1','CARGO_INCREMENTAL=0','CARGO_PROFILE_DEV_DEBUG=0','CARGO_PROFILE_TEST_DEBUG=0','RUST_TEST_THREADS=1','nice','-n','10','/usr/bin/time','-v','timeout','--signal=TERM','--kill-after=10s','600s',*command]
start=time.monotonic()
with (base/(name+'.log')).open('wb') as log:p=subprocess.run(wrapped,cwd=cwd,stdout=log,stderr=subprocess.STDOUT)
record={'name':name,'cwd':cwd,'command':command,'wrapped_command':wrapped,'exit_code':p.returncode,'elapsed_seconds':round(time.monotonic()-start,2),'finished_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'log':name+'.log','log_sha256':hashlib.sha256((base/(name+'.log')).read_bytes()).hexdigest()}
(base/(name+'.json')).write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record));print((base/(name+'.log')).read_text(errors='replace')[-3500:]);sys.exit(p.returncode)
