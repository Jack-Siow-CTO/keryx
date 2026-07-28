# Console follows messenger interaction principles, not a chat-app skin

Status: **accepted** (grill 2026-07-28). **Supersedes ADR 0020** (Slack dual-rail interaction shell as the Console interaction model).

Console copies useful **messenger** patterns—chat list home, Session-as-thread, Send-first idle composer, sticky in-thread Approvals, collapsible activity for tools/Child Runs—not pixel clones of WhatsApp/Telegram/Slack. Visual system remains original Keryx operator chrome (restrained neutrals, needs-you accent, system fonts). Domain seams stay intact: Worker SoR, Transcript vs Run events, one Active root Run per Session, no silent second root Run, no client-owned message queue. Rejected: dual-rail Slack shell as IA, consumer-chat simplicity that buries Approvals/Run activity, and IDE density theater inside chat chrome.
