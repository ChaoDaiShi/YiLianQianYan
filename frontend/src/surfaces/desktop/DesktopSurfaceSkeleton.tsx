import WorkspaceSurface from "../workspace/WorkspaceSurface";

export default function DesktopSurfaceSkeleton() {
  return (
    <div className="h-screen w-screen" data-surface-host="desktop-skeleton">
      <WorkspaceSurface />
    </div>
  );
}
