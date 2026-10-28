"""Run a native test command on a private Weston Wayland/XWayland display.

Usage: python scripts/with-private-display.py xwayland python scripts/check-native.py --backend x11 --skip-codex
The child suite must isolate its app data and clean up its own processes.
"""
import os,sys,tempfile,subprocess,pathlib,time,re,signal
if len(sys.argv) < 3 or sys.argv[1] not in ['wayland', 'xwayland']:
 raise SystemExit('Usage: with-private-display.py {wayland|xwayland} command [args...]')
backend=sys.argv[1]
root=pathlib.Path(tempfile.mkdtemp(prefix='ongleet-private-display-'))
runtime=root/'runtime';runtime.mkdir(mode=0o700)
env=dict(os.environ,XDG_RUNTIME_DIR=str(runtime))
env.pop('DISPLAY',None);env.pop('WAYLAND_DISPLAY',None)
with (root/'display.log').open('w') as log:
 display=subprocess.Popen(['weston','--backend=headless','--renderer=pixman','--width=1280','--height=900','--socket=validation','--idle-time=0','--no-config','--xwayland',f'--log={root}/weston.log'],env=env,stdout=log,stderr=log,start_new_session=True)
 try:
  deadline=time.monotonic()+15
  while time.monotonic()<deadline:
   text=(root/'weston.log').read_text() if (root/'weston.log').exists() else ''
   match=re.search(r'xserver listening on display (:\d+)',text,re.I)
   if (runtime/'validation').exists() and (backend=='wayland' or match):break
   time.sleep(.1)
  else:raise RuntimeError('Private compositor not ready: '+text)
  env['WAYLAND_DISPLAY']=str(runtime/'validation')
  env['GDK_BACKEND']='wayland' if backend=='wayland' else 'x11'
  if backend=='xwayland':env['DISPLAY']=match[1];env['ONGLEET_PRIVATE_X11']='1'
  print('Private display artifacts:',root,flush=True)
  result=subprocess.run(sys.argv[2:],env=env)
 finally:
  os.killpg(display.pid,signal.SIGTERM)
  try:display.wait(timeout=5)
  except subprocess.TimeoutExpired:os.killpg(display.pid,signal.SIGKILL);display.wait()
sys.exit(result.returncode)
