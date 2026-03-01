import asyncio

from vacht import Isolate


async def main():
    isolate = await Isolate.connect()
    value0 = await isolate.run("'hello world!'")
    print(value0)
    await value0.drop()

    value1 = await isolate.run("'hello world 2!'")
    print(value1)
    await value1.drop()

    await isolate.close()


asyncio.run(main())
