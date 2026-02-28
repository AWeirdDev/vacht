import asyncio

from vacht import Isolate


async def main():
    isolate = await Isolate.connect()
    await isolate.run("'hello world!'")
    await isolate.run("'hello world2!'")
    await isolate.close()


asyncio.run(main())
