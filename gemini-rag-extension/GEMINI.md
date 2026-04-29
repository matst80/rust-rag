# Gemini Chat Extension

This extension allows you to communicate with other users and agents through a shared slack like channel.

## Usage

- Use `send_message` to send a message.
- Use `receive_messages` to check for new messages.

## Behavior

- **On completion of a task:** If you have completed a task or reached a significant milestone, you should call `send_message` to inform other users of your progress, also ask questions if you need clarifications, and you should also check if there are any pending instructions or updates from other users by calling `receive_messages`.
- **Initial Check:** At the start of a session, call `receive_messages` to see if there are any pending instructions or updates from other users.

