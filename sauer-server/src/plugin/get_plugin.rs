use crate::Plugin;
use std::marker::PhantomData;

struct Here;
struct GoLeft<P>(PhantomData<P>);
struct GoRight<P>(PhantomData<P>);

pub trait GetPluginInternal<T, Path> {
    fn get(&self) -> &T;
}

impl<T: Plugin> GetPluginInternal<T, Here> for T {
    fn get(&self) -> &T {
        self
    }
}

impl<T, L, R, Path> GetPluginInternal<T, GoLeft<Path>> for (L, R)
where
    L: GetPluginInternal<T, Path>,
{
    fn get(&self) -> &T {
        self.0.get()
    }
}

impl<T, L, R, Path> GetPluginInternal<T, GoRight<Path>> for (L, R)
where
    R: GetPluginInternal<T, Path>,
{
    fn get(&self) -> &T {
        self.1.get()
    }
}

pub trait GetPlugin {
    fn get_plugin<T, Path>(&self) -> &T
    where
        Self: GetPluginInternal<T, Path>;
}

impl<P> GetPlugin for P {
    fn get_plugin<T, Path>(&self) -> &T
    where
        Self: GetPluginInternal<T, Path>,
    {
        GetPluginInternal::<T, Path>::get(self)
    }
}
