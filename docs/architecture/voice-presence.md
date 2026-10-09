# Voice, Presence and Narration

## Voice Core

Voice Core 是 Provider 中立的基础状态机，包含 VoiceSession、AudioInput/AudioOutput 抽象、STTProvider、TTSProvider、设备状态、VoiceProfile，以及 start、stop、interrupt、cancel、transcribe、speak。

当前仅装配 deterministic providers：STT 返回可预测文本，TTS 返回测试媒体字节。它们是 L1 Foundation 验证实现，不是实际模型能力。真实 Provider 后续以 trait adapter 接入，不得成为 Shared 架构依赖。

状态为 listening、speaking、interrupted、stopped、cancelled。终止状态再次操作会 fail closed；interrupt 同时通知输出 Provider 并更新 Presence。转录与语音生命周期产生 Product Event，但媒体字节从不进入 EventHub。

## Presence

Presence 不是单 enum，而是分层快照：

- activity：idle / working；
- interaction：none / listening / speaking / interrupted；
- attention：none / requested；
- source 与 updated_at。

这些维度可并存。`GET /api/presence` 和 `presence.get` 提供只读快照。

## Narration

Narration contract 将 Product Event 的聚合摘要映射为 Text/Voice delivery。Foundation 只提供 request/result、deterministic template fallback 与 `silent`、`balanced`、`companion` 三种 VoiceAttentionPolicy；不实现 LLM Narration 产品。
