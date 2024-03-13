from ivy.ivy import IvyServer
import time


class MyAgent(IvyServer):
    def __init__(self, agent_name):
        IvyServer.__init__(self,agent_name)
        self.start('127.255.255.255:2010')
        self.bind_msg(self.handle_hello, 'hello (.*)')

    def handle_hello(self, agent, *params):
        print(f"got {params} from {agent}")

if __name__ == '__main__':
  a=MyAgent('007')
  c = 0
  while True:
    msg = f"yo c{c}"
    msg2 = f"az a{c}"
    print(msg)
    a.send_msg(msg)
    a.send_msg(msg2)
    c += 1
    time.sleep(1)

