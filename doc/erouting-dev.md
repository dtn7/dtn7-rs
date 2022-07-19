# External Routing Developer Guide

# Notifications

``dtnd`` is using a general message bus through channels to deliver routing notifications. The sending is done by the ``routing_notify`` (``dtn7/src/lib.rs``) function. The External Routing needs to pass all received ``enum RoutingNotifcation`` from rust to the connected router. This is done by transforming them to JSON encode-able data structures before sending.

## Adding Notifications

If a new Notification is added it also needs to be added to the external routing.

### 1. New Notification was added

Let's imagine some kind of new notification was added to the enum.

```rust
pub enum RoutingNotifcation {
    SendingFailed(String, String),
    IncomingBundle(Bundle),
    IncomingBundleWithoutPreviousNode(String, String),
    EncounteredPeer(EndpointID),
    DroppedPeer(EndpointID),
    
    // Our new notification
    NewNotification(String, i32)
}
```

### 2. Add it to Packet enum

``dtn7/src/routing/erouting/mod.rs``

In order for the new notification to be available in the external routing we need to define a struct that will contain the data from the enum and then add it to the ``Packet`` enum.

```rust
#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum Packet {
    SenderForBundle(SenderForBundle),
    ResponseSenderForBundle(ResponseSenderForBundle),
    SendingFailed(SendingFailed),
    IncomingBundle(IncomingBundle),
    IncomingBundleWithoutPreviousNode(IncomingBundleWithoutPreviousNode),
    EncounteredPeer(EncounteredPeer),
    DroppedPeer(DroppedPeer),
    PeerState(PeerState),
    ServiceState(ServiceState),
    ServiceAdd(AddService),

    // Added to the Packet enum
    NewNotification(NewNotification)
}

/// Packet containing the Data of our new notification
#[derive(Serialize, Deserialize, Clone)]
pub struct NewNotification {
    pub some_text: String,
    pub some_number: i32,
}

```

### 3. Add to ``From<RoutingNotifiaction>`` Trait

``dtn7/src/routing/erouting/mod.rs``

Now that the ``Packet`` enum contains the new notification we need to add the conversion from the ``RoutingNotifcation`` to ``Packet`` in the ``from`` function. We need to add a new match in the function.

```rust
impl From<RoutingNotifcation> for Packet {
    fn from(notification: RoutingNotifcation) -> Self {
        match notification {
            // ... other matches ...

            // Our new notification
            RoutingNotifcation::NewNotification(some_text, some_number) => Packet::NewNotification(NewNotification {
                some_text: some_text,
                some_number: some_number,
            }),
        }
    }
}
```