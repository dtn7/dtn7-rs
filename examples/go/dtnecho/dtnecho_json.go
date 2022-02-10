package main

import (
	"encoding/json"
	"fmt"
	"io/ioutil"
	"log"
	"net/http"
	"net/url"
	"strings"

	"github.com/gorilla/websocket"
)

func checkErr(err error) {
	if err != nil {
		panic(err)
	}
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
	Data string `json:"data"`
}

// WsSendData dtn7-rs websocket sending data structure
type WsSendData struct {
	Src                  string `json:"src"`
	Dst                  string `json:"dst"`
	DeliveryNotification bool   `json:"delivery_notification"`
	Lifetime             uint64 `json:"lifetime"`
	Data                 string `json:"data"`
}

func main() {
	fmt.Println("== dtnecho (json mode) ==")

	u := url.URL{Scheme: "ws", Host: "127.0.0.1:3000", Path: "/ws"}
	log.Printf("[*] connecting to %s", u.String())

	c, _, err := websocket.DefaultDialer.Dial(u.String(), nil)
	checkErr(err)
	defer c.Close()

	// get the local node id
	err = c.WriteMessage(websocket.TextMessage, []byte("/node"))
	checkErr(err)

	_, message, err := c.ReadMessage()
	checkErr(err)
	log.Printf("[*] %s", message)

	reply_text := string(message)
	if !strings.HasPrefix(reply_text, "200 node: ") {
		panic("[*] unable to get node id via websocket")
	}
	nodeid := reply_text[10:]
	log.Printf("[*] node id: %s", nodeid)

	// register the service
	service := "echo"
	if strings.HasPrefix(nodeid, "ipn") {
		service = "7"
	}
	registerService(service)

	// set tx/rx mode to json so we don't need CBOR
	err = c.WriteMessage(websocket.TextMessage, []byte("/json"))
	checkErr(err)

	_, message, err = c.ReadMessage()
	checkErr(err)
	log.Printf("[*] %s", message)

	// subscribe to the registered service and start receiving bundles
	err = c.WriteMessage(websocket.TextMessage, []byte("/subscribe echo"))
	checkErr(err)

	_, message, err = c.ReadMessage()
	checkErr(err)
	log.Printf("[*] %s", message)

	// main loop to receive bundles and echo them back
	log.Printf("[*] entering main receive loop")
	for {
		mtype, message, err := c.ReadMessage()
		checkErr(err)
		if mtype == websocket.TextMessage {
			log.Printf("[<] %s", message)
		} else if mtype == websocket.BinaryMessage {
			log.Printf("recv: json data: %s", message)
			var data WsRecvData
			err = json.Unmarshal(message, &data)
			checkErr(err)
			log.Printf("[<] %s", data.Bid)
			//payload, _ := base64.StdEncoding.DecodeString(data.Data)
			//log.Printf("[<] data: %v", payload)

			response := WsSendData{
				Src:                  data.Dst,
				Dst:                  data.Src,
				DeliveryNotification: false,
				Lifetime:             3600 * 24 * 1000,
				Data:                 data.Data,
			}
			cborOut, err := json.Marshal(response)
			checkErr(err)
			//log.Printf("out: cbor data: %s", hex.EncodeToString(cborOut))
			err = c.WriteMessage(websocket.BinaryMessage, cborOut)
			checkErr(err)
		} else {
			log.Printf("[!] %v %s", mtype, message)
		}
	}
}
