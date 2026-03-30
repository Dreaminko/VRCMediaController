# Modular Avatar Guide

To receive media control signals from VRChat to the VRCMediaController application, you'll need to set up a prefab using Modular Avatar. This prefab will add parameters and menu controls to your avatar non-destructively.

## Prerequisites

1.  **VRChat SDK 3.0 (Avatars)** installed in your Unity project.
2.  **Modular Avatar** installed (via VCC).

## Step-by-Step Setup

1.  **Create an Empty GameObject** inside your Avatar's root object. Name it something like `Media Controls (MA)`.
2.  **Add Parameters Component**:
    *   Add the `MA Parameters` component to the new `Media Controls (MA)` object.
    *   Add three variables, set their types to `Bool`:
        -   `Media_PlayPause` (Default: False, Saved: False)
        -   `Media_Next` (Default: False, Saved: False)
        -   `Media_Prev` (Default: False, Saved: False)
3.  **Add Menu Component**:
    *   Create a new VRC Expressions Menu asset (e.g., `MediaMenu`).
    *   Add three controls to this menu:
        -   **Name**: `Play / Pause` | **Type**: Toggle | **Parameter**: `Media_PlayPause`
        -   **Name**: `Next Track` | **Type**: Button | **Parameter**: `Media_Next`
        -   **Name**: `Previous Track` | **Type**: Button | **Parameter**: `Media_Prev`
4.  **Install Menu via MA**:
    *   Add the `MA Menu Installer` component to the `Media Controls (MA)` object.
    *   Assign the `MediaMenu` asset you just created to the "Menu To Append" slot.

## Testing it Out

1.  Build and test the avatar in VRChat.
2.  Open your Action Menu locally. You should see the media controls.
3.  Ensure **OSC is Enabled** in the VRChat Action Menu (`Options -> OSC -> Enabled`).
4.  Run the VRCMediaController application on your PC.
5.  Click the Next/Prev buttons or toggle Play/Pause in VRChat to control Windows media!

## Viewing Track Data in VRChat

To test if playing music correctly updates your Chatbox, simply play music on your PC and check the status of your VRChat Chatbox above your head. You can optionally toggle the `Chatbox` inside the VRChat action menu under `Options -> OSC -> Chatbox`.
