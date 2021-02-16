package main

import (
	"fmt"
	"io/ioutil"
	"log"
	"net/http"
	"net/url"
	"strings"

	"github.com/fxamacker/cbor"
	"github.com/gorilla/websocket"
)

func checkErr(err error) {
	if err != nil {
		panic(err)
	}
}

func getNodeID() string {
	resp, err := http.Get("http://127.0.0.1:3000/status/nodeid")
	checkErr(err)
	defer resp.Body.Close()
	html, err := ioutil.ReadAll(resp.Body)
	checkErr(err)
	return string(html)
}
func registerService(service string) string {
	resp, err := http.Get("http://127.0.0.1:3000/register?" + service)
	checkErr(err)
	defer resp.Body.Close()
	html, err := ioutil.ReadAll(resp.Body)
	checkErr(err)
	return string(html)
}

// WsRecvData dtn7-rs websocket receiving data structure
type WsRecvData struct {
	Bid  string `json:"bid"`
	Src  string `json:"src"`
	Dst  string `json:"dst"`
	Data []byte `json:"data"`
}

// WsSendData dtn7-rs websocket sending data structure
type WsSendData struct {
	Src                  string `cbor:"src"`
	Dst                  string `cbor:"dst"`
	DeliveryNotification bool   `cbor:"delivery_notification"`
	Lifetime             uint64 `cbor:"lifetime"`
	Data                 []byte `cbor:"data"`
}

func main() {
	fmt.Println("== dtnecho ==")

	nodeid := getNodeID()

	service := "echo"
	if strings.HasPrefix(nodeid, "ipn") {
		service = "7"
	}
	registerService(service)

	u := url.URL{Scheme: "ws", Host: "127.0.0.1:3000", Path: "/ws"}
	log.Printf("[*] connecting to %s", u.String())

	c, _, err := websocket.DefaultDialer.Dial(u.String(), nil)
	checkErr(err)
	defer c.Close()

	err = c.WriteMessage(websocket.TextMessage, []byte("/data"))
	checkErr(err)

	_, message, err := c.ReadMessage()
	checkErr(err)
	log.Printf("[*] %s", message)

	err = c.WriteMessage(websocket.TextMessage, []byte("/subscribe echo"))
	checkErr(err)

	_, message, err = c.ReadMessage()
	checkErr(err)
	log.Printf("[*] %s", message)

	log.Printf("[*] entering main receive loop")
	for {
		mtype, message, err := c.ReadMessage()
		checkErr(err)
		if mtype == websocket.TextMessage {
			log.Printf("[<] %s", message)
		} else if mtype == websocket.BinaryMessage {
			//log.Printf("recv: cbor data: %s", hex.EncodeToString(message))
			var data WsRecvData
			err = cbor.Unmarshal(message, &data)
			checkErr(err)
			log.Printf("[<] %s", data.Bid)

			response := WsSendData{
				Src:                  data.Dst,
				Dst:                  data.Src,
				DeliveryNotification: false,
				Lifetime:             3600 * 24 * 1000,
				Data:                 data.Data,
			}
			cborOut, err := cbor.Marshal(response, cbor.CanonicalEncOptions())
			checkErr(err)
			//log.Printf("out: cbor data: %s", hex.EncodeToString(cborOut))
			err = c.WriteMessage(websocket.BinaryMessage, cborOut)
			checkErr(err)
		} else {
			log.Printf("[!] %v %s", mtype, message)
		}
	}
}
