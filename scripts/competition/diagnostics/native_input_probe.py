import ctypes,ctypes.wintypes as w,json,pathlib,subprocess,time,sys
root=pathlib.Path(sys.argv[1]).resolve()
helpers=pathlib.Path(__file__).resolve().parent
run=json.loads((root/'result.json').read_text(encoding='utf8'))
document=run['environment_before']['output_path']
observer=helpers.parent/'observe_notepad.ps1'
u=ctypes.WinDLL('user32',use_last_error=True)
class MI(ctypes.Structure):_fields_=[('dx',w.LONG),('dy',w.LONG),('mouseData',w.DWORD),('dwFlags',w.DWORD),('time',w.DWORD),('dwExtraInfo',ctypes.c_size_t)]
class KI(ctypes.Structure):_fields_=[('wVk',w.WORD),('wScan',w.WORD),('dwFlags',w.DWORD),('time',w.DWORD),('dwExtraInfo',ctypes.c_size_t)]
class U(ctypes.Union):_fields_=[('mi',MI),('ki',KI)]
class I(ctypes.Structure):_fields_=[('type',w.DWORD),('u',U)]
def send(events):
 values=(I*len(events))(*[I(1,U(ki=KI(*event))) for event in events]);sent=u.SendInput(len(values),values,ctypes.sizeof(I));assert sent==len(values),(sent,ctypes.get_last_error())
def observe(phase):
 subprocess.run(['powershell.exe','-NoProfile','-File',str(observer),'-OutputDirectory',str(root),'-Phase',phase,'-ExpectedDocument',document],check=True,stdout=subprocess.DEVNULL,creationflags=0x08000000)
 value=json.loads((root/(phase+'-observation.json')).read_text(encoding='utf8'));assert len(value['windows'])==1 and value['windows'][0]['document_matches_expected'];return value['windows'][0]['controls'][0]['text']
text='星桥 | 负责人林岚 | 2026-11-04 | B305 | NATIVE-ROUNDTRIP'
results=[]
for mode in ['batch','paced']:
 subprocess.run(['powershell.exe','-NoProfile','-File',str(helpers/'focus_test_document.ps1'),'-ExpectedDocument',document],check=True,creationflags=0x08000000)
 send([(0x11,0,0,0,0),(0x41,0,0,0,0),(0x41,0,2,0,0),(0x11,0,2,0,0)])
 time.sleep(.15)
 events=[]
 for unit in memoryview(text.encode('utf-16-le')).cast('H'):
  pair=[(0,unit,4,0,0),(0,unit,6,0,0)]
  if mode=='paced':send(pair);time.sleep(.01)
  else:events.extend(pair)
 if mode=='batch':send(events)
 time.sleep(.5)
 actual=observe('native-'+mode)
 results.append({'mode':mode,'expected':text,'actual':actual,'matches':actual==text})
(root/'native-probe.json').write_text(json.dumps(results,ensure_ascii=False,indent=2),encoding='utf8')
print(json.dumps(results,ensure_ascii=True))
