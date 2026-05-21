# Matrix Client Feature Checklist

Legend:
- ✔️ Supported
- ❌ Not Planned
- Empty = Planned / Work in Progress

# Account & Session Management

- [✔️] Login via password
- [] Login via SSO
- [❌] Login via token
- [✔️] Registration
- [✔️] Logout
- [✔️] Multi-account support
- [✔️] Session restore
- [✔️] Device management
- [✔️] Cross-signing
- [✔️] Interactive verification (SAS)
- [❌] QR code verification
- [✔️] Secret storage (SSSS)
- [] Dehydrated devices
- [✔️] Soft logout handling
- [] Account deactivation

# Room Management

- [] Create rooms
- [✔️] Join rooms
- [] Leave rooms
- [] Knock on rooms
- [✔️] Invite users
- [] Ban users
- [] Kick users
- [] Ignore users
- [] Room upgrades
- [] Space support
- [] Nested spaces
- [✔️] Direct messages
- [✔️] Public room directory
- [] Room aliases
- [] Room topic editing
- [] Room avatar editing
- [] Room permissions editing
- [] Room history visibility settings
- [] Guest access settings
- [] Tombstone handling
- [] Restricted rooms

# Messaging

- [✔️] Send text messages
- [✔️] Edit messages
- [✔️] Delete/redact messages
- [✔️] Reply to messages
- [] Threaded conversations
- [] Reactions
- [] Polls
- [] Typing indicators
- [] Read receipts
- [] Fully read markers
- [] Presence support
- [✔️] Markdown formatting
- [] Code block rendering
- [✔️] Message search
- [] Mentions
- [] Room mentions
- [✔️] Message permalinks
- [] URL previews
- [] Stickers
- [] Emojis / custom emoji packs
- [] Voice messages
- [] Location messages
- [] Scheduled messages
- [] Message pinning
- [] Message forwarding
- [] Message translation
- [✔️] Unread counters
- [] Notification badges
- [] Notification customization
- [] Silent notifications
- [] Push notifications
- [] Local notifications


# Media

- [] Image upload
- [] Video upload
- [] Audio upload
- [] File upload
- [] Clipboard paste upload
- [] Drag and drop upload
- [] Media previews
- [] Image galleries
- [] Animated GIF support
- [] Blurhash thumbnails
- [] Media compression
- [] Media streaming
- [] Progressive media loading
- [] Media caching
- [] Encrypted media support
- [] Inline video playback
- [] Inline audio playback


# Encryption (E2EE)

- [✔️] Megolm room encryption
- [✔️] Olm device encryption
- [✔️] Encrypted DMs
- [✔️] Encrypted group chats
- [✔️] Key backup
- [✔️] Key export/import
- [✔️] Secure key sharing
- [✔️] Automatic session recovery
- [] Unverified device warnings
- [] Device trust management
- [] Encryption status indicators
- [✔️] Verification requests
- [✔️] Recovery key support
- [✔️] Recovery passphrase support


# VoIP & Calls

- [] 1:1 voice calls
- [] 1:1 video calls
- [] Group voice calls
- [] Group video calls
- [] Screen sharing
- [] Call transfer
- [] Call recording
- [] Push-to-talk
- [] Spatial audio
- [] Call encryption
- [] TURN server support
- [] Call notifications


# Sync & Offline

- [✔️] Sliding Sync
- [❌] Incremental sync
- [✔️] Background sync
- [✔️] Offline message cache
- [] Offline sending queue
- [✔️] Local timeline persistence
- [] Lazy loading members
- [✔️] Partial room loading
- [✔️] Timeline pagination
- [✔️] Sync recovery handling
- [✔️] Sync error recovery
- [] Presence sync
- [] Typing sync
# Profiles & Social Features

- [] User profiles
- [] Avatar support
- [] Status messages
- [] Presence states
- [] Profile search
- [] Mutual rooms display
- [❌] Contact list integration
- [] Third-party identifier support
- [✔️] User directory search

# Moderation & Safety

- [] Report messages
- [] Report users
- [] Block users
- [] Keyword filters
- [] NSFW media blur
- [] Moderator tools
- [] Power level management
- [] Room ACL support
- [] Server ACL support
- [] Audit logs
- [] Anti-spam tools

# Client Customization

- [✔️] Themes
- [✔️] Dark mode
- [❌] AMOLED mode
- [] Custom accent colors
- [] Font scaling
- [✔️] Mobile layout
- [✔️] Tablet layout
- [✔️] Desktop layout
- [❌] Multi-window support
- [] Accessibility support
- [] Keyboard shortcuts
- [] Custom notification sounds

# Platform Features

- [✔️] Windows support
- [✔️] Linux support
- [✔️] macOS support
- [✔️] Android support
- [✔️] iOS support
- [❌] Web support
- [✔️] aarch64 support (for android also armv7)
- [✔️] x86_64 support (excluding iOS, for android also x86)
- [❌] Riscv64 support 

# Experimental / Advanced Features

- [✔️] Custom homeserver discovery
- [✔️] End-to-end encrypted search
- [❌] Multi-session merging
- [] Custom event rendering
- [] Plugin system
- [❌] Scripting support
- [] Developer tools
- [] Protocol inspector
- [] Timeline debugging tools
