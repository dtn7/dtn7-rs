#!/usr/bin/env scala-cli

// to install scala-cli see https://scala-cli.virtuslab.org/

//> using scala "3.1.2"
//> using lib "com.softwaremill.sttp.client3::core::3.6.2"
//> using lib "com.softwaremill.sttp.client3::okhttp-backend::3.6.2"
//> using lib "com.github.plokhotnyuk.jsoniter-scala::jsoniter-scala-core::2.13.31"
//> using lib "com.github.plokhotnyuk.jsoniter-scala::jsoniter-scala-macros::2.13.31"

import com.github.plokhotnyuk.jsoniter_scala.core.{JsonValueCodec, readFromArray, writeToArray}
import com.github.plokhotnyuk.jsoniter_scala.macros.JsonCodecMaker
import sttp.capabilities.WebSockets
import sttp.client3.*
import sttp.client3.okhttp.OkHttpSyncBackend
import sttp.model.Uri
import sttp.ws.{WebSocket, WebSocketFrame}

import java.nio.charset.StandardCharsets
import java.util.Base64

/** API base path used for http request */
val api: String =
  val ip   = "127.0.0.1"
  val port = 3000
  s"http://$ip:$port"

val backend: SttpBackend[Identity, WebSockets] = OkHttpSyncBackend()

/** get uri body as string, throwing on any errors */
def uget(uri: Uri): String = backend.send(basicRequest.get(uri).response(asStringAlways)).body

// what follows are the data type and codec definitions for receiving and sending bundles
case class ReceivedBundle(bid: String, src: String, dst: String, data: String):
  def payload: Array[Byte] = Base64.getDecoder.decode(data)
case class SendBundle(src: String, dst: String, data: String, delivery_notification: Boolean, lifetime: Long)
given JsonValueCodec[ReceivedBundle] = JsonCodecMaker.make
given JsonValueCodec[SendBundle]     = JsonCodecMaker.make

@main def run(): Unit =
  val local_node = uget(uri"$api/status/nodeid")
  println(s"Running echo service on $local_node")
  // Define service endpoint, "echo" for 'dtn' nodes and '7' for 'ipn' nodes
  val service =
    if local_node.startsWith("ipn")
    then "7"
    else "echo"
  // Prior to receiving anything register the local service endpoint
  val register = uget(uri"$api/register?$service")
  println(s"registration message: $register")

  // this uses synchronous websocket for demo purposes, which is a bad idea in case of concurrent messages
  val ws = backend.send(basicRequest.get(uri"$api/ws").response(asWebSocketAlwaysUnsafe)).body

  def echoConfirmation(): Unit = println(ws.receiveText())

  // select json communication
  ws.sendText("/json")
  echoConfirmation()

  // ask to receive messages on the the given path
  ws.sendText(s"/subscribe $service")
  echoConfirmation()

  while true do
    // read incoming bundles
    val msg = readFromArray[ReceivedBundle](ws.receiveBinary(true))
    println(s"echoing ${new String(msg.payload)}")

    // send them back to the receiver
    ws.sendBinary(writeToArray(SendBundle(
      // change src and dst
      src = msg.dst,
      dst = msg.src,
      lifetime = 3600 * 24 * 1000,
      delivery_notification = false,
      data = msg.data
    )))
    echoConfirmation()
