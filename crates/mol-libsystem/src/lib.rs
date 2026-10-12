// SPDX-License-Identifier: MIT OR Apache-2.0
//! relibc-style libSystem reimplementation (docs 07, 12 L2).
//!
//! Contract: init order is law — kernel → platform → pthread →
//! libc → malloc → dyld → dispatch → xpc, verified by traced
//! launch (P1.2.1/P2.2.V4). Guest-visible quirks are handrolled
//! behind trait seams (doc 23).
