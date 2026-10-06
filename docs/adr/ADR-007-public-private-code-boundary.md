# ADR 007 Public and Private Code Boundary

状态：Accepted
日期：2026-10-06

## Decision

1. YiLianCloud server implementation is private.
2. No private server or Admin source may enter the public YiLianQianYan repository.
3. The public repository contains only public cloud contracts and client adapters.
4. Private source lives outside the public Git root.
5. `.gitignore` is not considered a security boundary.
6. Deployment uses built artifacts rather than a GitHub source checkout.
7. Admin API is never part of the public application contract.
8. Secrets never enter either repository or a deployment package in plaintext.

The private repository is `F:\项目开发\忆涟千言\YiLianCloud-Private`, uses a distinct local Git root and has no remote by default. Shared material is limited to versioned contracts or generated clients; source directories are never linked.

## Enforcement

The public repository runs `scripts/check-private-boundary.ps1` locally and in CI. It rejects private implementation paths and Admin route strings in public executable source. Release review additionally checks the private repository remote list, public tracked files and deployment archive contents.
