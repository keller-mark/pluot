import React, { useState, useMemo } from 'react';
import { Another } from './Another.jsx';


export function App(props) {


    return (
        <div>
            <p>Hello from App component!</p>
            <Another />
        </div>
    );
}