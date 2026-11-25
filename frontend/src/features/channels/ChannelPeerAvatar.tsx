import { useState } from "react";
import { UserRound, UsersRound } from "lucide-react";

export function qqAvatarUrl(id: string, group = false) {
  if (!/^[1-9]\d*$/.test(id)) return undefined;
  return group
    ? `https://p.qlogo.cn/gh/${id}/${id}/100`
    : `https://q1.qlogo.cn/g?b=qq&nk=${id}&s=100`;
}

export default function ChannelPeerAvatar({ group = false, src }: { group?: boolean; src?: string }) {
  const [failedSrc, setFailedSrc] = useState<string>();
  return <span className="channel-session-avatar" aria-hidden="true">
    {src && src !== failedSrc
      ? <img src={src} alt="" loading="lazy" referrerPolicy="no-referrer" onError={() => setFailedSrc(src)} />
      : group ? <UsersRound size={16} /> : <UserRound size={16} />}
  </span>;
}
