# Window metadata only. Never query titles, descendants, UIA, pixels or messages.
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$targetProcess = Get-Process -Id ([int]$env:YILIAN_TEST_WINDOW_PID)
if (-not [string]::Equals($targetProcess.Path, $env:YILIAN_TEST_WINDOW_EXE, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'The selected process executable changed'
}
Add-Type -TypeDefinition @'
using System; using System.Text; using System.Runtime.InteropServices;
public static class SoftwareWindowMetadata {
 [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left,Top,Right,Bottom; }
 [StructLayout(LayoutKind.Sequential)] public struct Placement { public int Length,Flags,ShowCmd,MinX,MinY,MaxX,MaxY; public Rect Normal; }
 [StructLayout(LayoutKind.Sequential)] public struct Point { public int X,Y; }
 [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h,out uint pid);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr h,StringBuilder c,int n);
 [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
 [DllImport("user32.dll")] static extern bool IsIconic(IntPtr h);
 [DllImport("user32.dll")] static extern bool IsChild(IntPtr parent,IntPtr child);
 [DllImport("user32.dll")] static extern IntPtr WindowFromPoint(Point p);
 [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h,out Rect r);
 [DllImport("user32.dll")] static extern IntPtr GetAncestor(IntPtr h,uint f);
 [DllImport("user32.dll")] static extern IntPtr GetDesktopWindow();
 [DllImport("user32.dll")] static extern IntPtr GetWindow(IntPtr h,uint f);
 [DllImport("user32.dll")] static extern IntPtr GetWindowLongPtr(IntPtr h,int k);
 [DllImport("user32.dll")] static extern IntPtr SetThreadDpiAwarenessContext(IntPtr mode);
 [DllImport("user32.dll")] static extern uint GetDpiForWindow(IntPtr h);
 [DllImport("user32.dll")] static extern bool GetWindowPlacement(IntPtr h,ref Placement p);
 [DllImport("user32.dll")] static extern IntPtr SendMessageTimeout(IntPtr h,uint m,IntPtr w,IntPtr l,uint flags,uint timeout,out UIntPtr r);
 public static object Read(long raw,uint expectedPid) {
  SetThreadDpiAwarenessContext(new IntPtr(-4));
  var h=new IntPtr(raw); uint pid; GetWindowThreadProcessId(h,out pid); if(pid!=expectedPid)throw new Exception("The selected HWND changed process");
  var cls=new StringBuilder(256);GetClassName(h,cls,cls.Capacity);Rect rect;if(!GetWindowRect(h,out rect))throw new Exception("Window rectangle unavailable");
  var placement=new Placement{Length=Marshal.SizeOf(typeof(Placement))};if(!GetWindowPlacement(h,ref placement))throw new Exception("Window placement unavailable");
  var parent=GetAncestor(h,1);uint parentPid;GetWindowThreadProcessId(parent,out parentPid);UIntPtr answer;
  var timer=System.Diagnostics.Stopwatch.StartNew();
  var responsive=SendMessageTimeout(h,0,IntPtr.Zero,IntPtr.Zero,3,1500,out answer)!=IntPtr.Zero;
  timer.Stop();
  var xy=new Point{X=(rect.Left+rect.Right)/2,Y=(rect.Top+rect.Bottom)/2};
  var packedPoint=new IntPtr((xy.X&0xffff)|((xy.Y&0xffff)<<16));
  var hit=WindowFromPoint(xy);
  var ownsCenter=hit==h||IsChild(h,hit); UIntPtr hitCode;
  var clientHit=ownsCenter&&SendMessageTimeout(hit,0x84,IntPtr.Zero,packedPoint,3,500,out hitCode)!=IntPtr.Zero&&hitCode.ToUInt64()==1;
  return new {clientHit=clientHit,hwnd=raw.ToString(),pid=pid,className=cls.ToString(),visible=IsWindowVisible(h),minimized=IsIconic(h),parent=parent.ToInt64().ToString(),parentPid=parentPid,desktopParent=parent==GetDesktopWindow(),owner=GetWindow(h,4).ToInt64().ToString(),style=GetWindowLongPtr(h,-16).ToInt64(),exstyle=GetWindowLongPtr(h,-20).ToInt64(),dpi=GetDpiForWindow(h),rect=rect,placement=placement,responds=responsive,responseMs=timer.Elapsed.TotalMilliseconds,ownsCenter=ownsCenter};
 }
}
'@
[SoftwareWindowMetadata]::Read([long]$env:YILIAN_TEST_WINDOW_HANDLE, [uint32]$targetProcess.Id) | ConvertTo-Json -Depth 5 -Compress
