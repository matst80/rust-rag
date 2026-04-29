package acp

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"os"
	"os/exec"
	"sync"
	"sync/atomic"
)

// StdioConnection is a JSON-RPC 2.0 framing layer over a child process's
// stdin/stdout. Each message is one line of JSON terminated with '\n'.
type StdioConnection struct {
	cmd    *exec.Cmd
	stdin  io.WriteCloser
	stdout io.ReadCloser
	stderr io.ReadCloser

	nextID    uint64
	pendingMu sync.Mutex
	pending   map[uint64]chan *Message
	handler   func(msg Message)
	writeMu   sync.Mutex

	cancel    context.CancelFunc
	closeOnce sync.Once
	debug     bool
}

func NewStdioConnection(ctx context.Context, name, command string, args []string, debug bool) (*StdioConnection, error) {
	cmdCtx, cancel := context.WithCancel(ctx)
	cmd := exec.CommandContext(cmdCtx, command, args...)
	cmd.Env = append(cmd.Env, os.Environ()...)

	stdin, err := cmd.StdinPipe()
	if err != nil {
		cancel()
		return nil, err
	}
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		cancel()
		return nil, err
	}
	stderr, err := cmd.StderrPipe()
	if err != nil {
		cancel()
		return nil, err
	}
	if err := cmd.Start(); err != nil {
		cancel()
		return nil, err
	}

	c := &StdioConnection{
		cmd:     cmd,
		stdin:   stdin,
		stdout:  stdout,
		stderr:  stderr,
		pending: make(map[uint64]chan *Message),
		cancel:  cancel,
		debug:   debug,
	}

	go func() {
		streamPrefixedLines(os.Stderr, stderr, fmt.Sprintf("[%s stderr]", name))
	}()
	go func() {
		_ = cmd.Wait()
		log.Printf("[%s] process exited", name)
		c.Close()
	}()
	go c.listen(name)

	return c, nil
}

func streamPrefixedLines(dst io.Writer, src io.Reader, prefix string) {
	scanner := bufio.NewScanner(src)
	scanner.Buffer(make([]byte, 0, 1024), 1024*1024)
	for scanner.Scan() {
		_, _ = fmt.Fprintf(dst, "%s %s\n", prefix, scanner.Text())
	}
	if err := scanner.Err(); err != nil {
		log.Printf("%s read err: %v", prefix, err)
	}
}

func (c *StdioConnection) SetHandler(h func(msg Message)) { c.handler = h }

func (c *StdioConnection) Close() error {
	c.closeOnce.Do(func() {
		c.cancel()
		if c.stdin != nil {
			_ = c.stdin.Close()
		}
		if c.stdout != nil {
			_ = c.stdout.Close()
		}
		if c.stderr != nil {
			_ = c.stderr.Close()
		}
	})
	return nil
}

func (c *StdioConnection) Call(ctx context.Context, method string, params any) (*Message, error) {
	id := atomic.AddUint64(&c.nextID, 1)
	body, _ := json.Marshal(params)
	ch := make(chan *Message, 1)
	c.pendingMu.Lock()
	c.pending[id] = ch
	c.pendingMu.Unlock()

	if err := c.send(Message{Method: method, Params: body, ID: id}); err != nil {
		c.pendingMu.Lock()
		delete(c.pending, id)
		c.pendingMu.Unlock()
		return nil, err
	}
	select {
	case <-ctx.Done():
		c.pendingMu.Lock()
		delete(c.pending, id)
		c.pendingMu.Unlock()
		return nil, ctx.Err()
	case resp, ok := <-ch:
		if !ok {
			return nil, fmt.Errorf("connection closed")
		}
		if resp.Error != nil {
			return nil, fmt.Errorf("rpc error %d: %s", resp.Error.Code, resp.Error.Message)
		}
		return resp, nil
	}
}

func (c *StdioConnection) Notify(_ context.Context, method string, params any) error {
	body, _ := json.Marshal(params)
	return c.send(Message{Method: method, Params: body})
}

func (c *StdioConnection) Respond(id any, result any, rpcErr *RPCError) error {
	msg := Message{ID: id}
	if rpcErr != nil {
		msg.Error = rpcErr
	} else {
		body, _ := json.Marshal(result)
		msg.Result = body
	}
	return c.send(msg)
}

func (c *StdioConnection) send(msg Message) error {
	msg.JSONRPC = "2.0"
	data, err := json.Marshal(msg)
	if err != nil {
		return err
	}
	if c.debug {
		log.Printf("[acp >] %s", string(data))
	}
	data = append(data, '\n')
	c.writeMu.Lock()
	defer c.writeMu.Unlock()
	_, err = c.stdin.Write(data)
	return err
}

func (c *StdioConnection) listen(name string) {
	defer func() {
		c.pendingMu.Lock()
		for id, ch := range c.pending {
			close(ch)
			delete(c.pending, id)
		}
		c.pendingMu.Unlock()
	}()
	scanner := bufio.NewScanner(c.stdout)
	scanner.Buffer(make([]byte, 0, 1024*1024), 4*1024*1024)
	for scanner.Scan() {
		line := scanner.Bytes()
		if c.debug {
			log.Printf("[acp <] %s", string(line))
		}
		var msg Message
		if err := json.Unmarshal(line, &msg); err != nil {
			log.Printf("[%s] unmarshal: %v", name, err)
			continue
		}
		if msg.ID != nil && msg.Method == "" {
			// response to a Call
			var id uint64
			switch v := msg.ID.(type) {
			case float64:
				id = uint64(v)
			case int64:
				id = uint64(v)
			default:
				continue
			}
			c.pendingMu.Lock()
			ch, ok := c.pending[id]
			delete(c.pending, id)
			c.pendingMu.Unlock()
			if ok {
				ch <- &msg
			}
			continue
		}
		if c.handler == nil {
			continue
		}
		// Requests from the agent (have both ID and Method) need a goroutine
		// because we'll Respond back via the same connection — running inline
		// would deadlock the listener. Notifications run inline so chunk
		// streams stay in arrival order without inter-goroutine races.
		if msg.ID != nil && msg.Method != "" {
			go c.handler(msg)
		} else {
			c.handler(msg)
		}
	}
}
