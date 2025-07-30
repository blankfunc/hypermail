# HyperMail Core
In fact, for various design methods, the internal message transmission path is roughly as follows:

``` mermaid
graph TD;
	IO --> Link;
	Link --> IO;
	Link --> Session
	Link --> Stream
	Session --> Stream
	Session --> Link
	Stream --> Link
```

In [packt.rs](./packet.rs), we define the messages for all internal channels, and also follow the above relationship.

As you can see:

+ `IO` is responsible for sending and receiving data to the user's abstract encapsulation layer, as well as parsing the received data and broadcasting it to the lower layers.
+ `Link` is the **logical center** where decisions regarding connection disconnections, packet timeouts, and heartbeat packets are made. Simultaneously responsible for serializing data packets transmitted from lower layers, and then passing binary data to `IO`.
+ `Session` and `Stream` are both project definitions, with the former only used for identification and the latter used for transmitting stream data.

In the design, a total of 4 handshakes are required from the beginning of the connection to sending data.

```
Client> SessionOpen
Server> SessionOpenAck
Client> StreamOpen
Server> StreamOpenAck
```

Of course, not using streaming messages would be faster.

```
Client> SessionOpen
Server> SessionOpenAck
Client> StreamBlock
```

## Protocol

### Session

| Name               | Description                                         |
| ------------------ | --------------------------------------------------- |
| `Open` | Attempt to request session opening |
| `OpenAck` | Response to session creation request |
| `Reopen` | Attempt to request session reconnection |
| `ReopenAck` | Response indicating whether reconnection is allowed |
| `Close` | Request to close the session |
| `CloseAck` | Response indicating readiness to close the session |
| `Death` | Forcefully terminate the session |

### Stream

| Name              | Description                                                |
| ----------------- | ---------------------------------------------------------- |
| `Block`     | Whole data packet                                          |
| `BlockAck`  | Acknowledgment of received data packet                     |
| `Open`      | Request to create a stream                                 |
| `OpenAck`   | Response indicating whether stream creation is allowed     |
| `Reopen`    | Request to reconnect a stream                              |
| `ReopenAck` | Response indicating whether stream reconnection is allowed |
| `Chunk`     | Send a chunk of data on the stream                         |
| `ChunkAck`  | Acknowledgment of received data chunk                      |
| `Flush`     | Notify that all data has been sent                         |
| `FlushAck`  | Acknowledgment that all data has been received             |
| `Lack`      | Response indicating missing data                           |
| `Later`     | Request to delay sending                                   |
| `Go`        | Request to continue sending                                |
| `Clear`     | Abnormally terminate the stream                            |

### Health

| Name         | Description                         |
| ------------ | ----------------------------------- |
| `Ping` | Client heartbeat packet             |
| `Pong` | Server response to heartbeat packet |
