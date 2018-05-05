# SimpleFileSystem Rust移植报告

王润基 2018.05.05

## 任务目标

* 用Rust重新实现SFS模块，力求精简
* 以crate库的形式发布，不依赖OS，支持单元测试
* 导出兼容ucore的C接口，能链接到ucore lab8中替换原有实现
* 在RustOS中使用

## 结构设计

自底向上：

#### 硬盘数据结构层

和SFS在硬盘上具体的存储结构相关。位于`structs.rs`。

这部分定义了硬盘上的数据结构，完全照搬C中的定义，并实现了一些Rust辅助方法。

#### SFS层

和SFS在内存中的对象相关。位于`sfs.rs`。

这部分主要定义了两个对象：`SimpleFileSystem`和`INode`。

它们依赖下层的具体结构，实现上层VFS的接口，完成具体的文件操作。

由于利用了Rust的一些模块的特性，这两个结构体的定义和C中很不一样。

#### VFS层

文件系统通用接口。位于`vfs.rs`。

这部分主要定义了三个接口：`FileSystem`，`INode`，`Device`。

其中前两个是需要具体文件系统实现的。Device是依赖项，提供读写方法。

它们基本照搬C中的定义，但换成了Rust风格，更加本质。

由于C在语言层面缺乏对接口的支持，ucore中是用struct和函数指针实现接口功能。但在Rust中就是简单的Trait。

#### C兼容层

将Rust风格的VFS导出为ucore可用的C接口。位于`c_interface.rs`。

这部分导入了ucore中`stat` `iobuf` `device`等结构，将他们实现Rust的Trait。同时将VFS层中的Trait转化成C函数接口。

除此之外，还要导入ucore中的一些基础设施，例如`kmalloc` `kfree` `cprintf` `panic`

## Rust带来的好处

* 利用RAII特性，使用一些小Wrapper结构，进行自动标记和断言，大大减轻开发者心智负担。例如：Mutex锁，RefCell访问检查，Dirty脏标记，Rc引用计数。
* 从始至终严格的访问控制，只要不滥用unsafe，可快速并行化，并保证安全。
* 语言描述能力强，代码量少。对比C的SFS模块1100+行，Rust只用了700+行（除去单元测试和C兼容层）。

## 接下来的目标

* 接入ucore中能够跑起来
* 实现mksfs等周边工具
* 多线程支持及并行优化